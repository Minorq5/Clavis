<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { fade, slide } from "svelte/transition";
  import * as Y from "yjs";
  import { api, listen, type Folder, type NoteMeta, type Reminder, type SharedMeta, type Template, type VersionMeta } from "../lib/api";
  import { t, ago, i18n } from "../lib/i18n.svelte";
  import { search } from "../lib/fuzzy";
  import { toDocx } from "../lib/docx";
  import { toMarkdown } from "../lib/markdown";
  import { docToHtml } from "../lib/html";
  import { collectDeadlines, formatDeadline } from "../lib/deadline";
  import { builtInTemplates } from "../lib/templates";
  import type { CommentData } from "../lib/comment";
  import { toast } from "../lib/toast.svelte";
  import { ask, askText } from "../lib/dialog.svelte";
  import { openMenu, type MenuItem } from "../lib/menu.svelte";
  import { nav, go } from "../lib/nav.svelte";
  import { lowerCustom, type LinkTarget } from "../lib/nodes";
  import type { SharedMember } from "../lib/api";
  import { sharedError } from "../lib/sharedError";
  import { ClavisProvider, colorFor, fromB64, type Peer } from "../lib/collab";
  import Icon from "../lib/Icon.svelte";
  import Modal from "../lib/Modal.svelte";
  import Editor from "./Editor.svelte";
  import ShareModal from "./ShareModal.svelte";

  type Current = { kind: "personal"; meta: NoteMeta } | { kind: "shared"; meta: SharedMeta };
  type Filter = "all" | "none" | "trash" | string;

  let notes = $state<NoteMeta[]>([]);
  let shared = $state<SharedMeta[]>([]);
  let folders = $state<Folder[]>([]);
  let templates = $state<Template[]>([]);
  let reminders = $state<Reminder[]>([]);
  let query = $state("");
  let filter = $state<Filter>("all");
  let current = $state<Current | null>(null);
  let doc = $state<unknown>(null);
  let locked = $state(false);
  let editorKey = $state(0);
  let provider = $state<ClavisProvider | null>(null);
  let me = $state<{ name: string; color: string }>({ name: "", color: "#8fb3d9" });
  let opening = $state(false);
  let peers = $state<Peer[]>([]);
  let online = $state(true);
  let myRole = $state<"owner" | "editor">("editor");
  let ed = $state<ReturnType<typeof Editor>>();
  let savedAt = $state(0);
  let dirty = $state(false);

  let pending: { doc: unknown; title: string } | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let titleTimer: ReturnType<typeof setTimeout> | undefined;
  let dueTimer: ReturnType<typeof setTimeout> | undefined;
  const lastDue = new Map<string, string>();
  let busy = $state(false);

  // окна
  let joinOpen = $state(false);
  let joinLink = $state("");
  let joinErr = $state("");
  let joining = $state(false);
  let shareOpen = $state(false);
  let secretOpen = $state<"set" | null>(null);
  let pw1 = $state("");
  let pw2 = $state("");
  let pwErr = $state("");
  let unlockPw = $state("");
  let unlockErr = $state("");
  let versionsOpen = $state(false);
  let versions = $state<VersionMeta[]>([]);
  let vSel = $state<{ id: string; doc: unknown } | null>(null);
  let tagInput = $state<string | null>(null);

  // комментарии
  let commentsOpen = $state(false);
  let comments = $state<CommentData[]>([]);
  let draft = $state<{ id: string; quote: string; text: string } | null>(null);
  let replyTo = $state<string | null>(null);
  let replyText = $state("");
  let showResolved = $state(false);
  let flashed = $state<string | null>(null);
  let commentsMap: Y.Map<unknown> | null = null;

  const live = $derived(notes.filter((n) => !n.trashed));
  const trash = $derived(notes.filter((n) => n.trashed).sort((a, b) => b.trashed - a.trashed));
  const allTags = $derived([...new Set(live.flatMap((n) => n.tags))].sort((a, b) => a.localeCompare(b)));

  function matches<T>(items: T[], text: (x: T) => string, tags: (x: T) => string[]): T[] {
    const q = query.trim();
    if (q.startsWith("#")) {
      const tag = q.slice(1).toLocaleLowerCase();
      return items.filter((x) => tags(x).some((g) => g.toLocaleLowerCase().startsWith(tag)));
    }
    return search(items, q, text);
  }

  const inFilter = $derived(filter === "trash" ? trash : live.filter((n) => (filter === "all" ? true : filter === "none" ? !n.folder : n.folder === filter)));
  const visibleNotes = $derived(matches(inFilter, (n) => `${n.title} ${n.tags.join(" ")} ${n.text}`, (n) => n.tags));
  const pinnedNotes = $derived(filter === "trash" ? [] : visibleNotes.filter((n) => n.pinned));
  const otherNotes = $derived(filter === "trash" ? visibleNotes : visibleNotes.filter((n) => !n.pinned));
  const sharedNotes = $derived(shared.filter((s) => s.kind === "note"));
  const visibleShared = $derived(
    filter === "all" ? matches(sharedNotes, (s) => s.title, () => []).sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updated - a.updated) : [],
  );
  const filterLabel = $derived(
    filter === "all" ? t("nt.all") : filter === "none" ? t("nt.noFolder") : filter === "trash" ? t("nav.trash") : folders.find((f) => f.id === filter)?.name ?? t("nt.all"),
  );
  const upcoming = $derived.by(() => {
    const now = Date.now();
    const known = new Set([...live.map((n) => n.id), ...sharedNotes.map((s) => s.id)]);
    return reminders
      .filter((r) => known.has(r.note) && r.at > now - 86_400_000 && r.at < now + 14 * 86_400_000)
      .sort((a, b) => a.at - b.at)
      .slice(0, 3);
  });
  const openComments = $derived(comments.filter((c) => !c.resolved));
  const resolvedComments = $derived(comments.filter((c) => c.resolved));
  const linkTargets = $derived<LinkTarget[]>([
    ...live.filter((n) => !n.secret).map((n) => ({ id: n.id, kind: "note" as const, title: n.title || t("lib.untitled") })),
    ...sharedNotes.map((s) => ({ id: s.id, kind: "shared" as const, title: s.title || t("lib.untitled") })),
  ].filter((l) => l.id !== current?.meta.id));
  const openLink = (tg: { id: string; kind: "note" | "shared" }) => openTarget(tg);
  let members = $state<SharedMember[]>([]);
  let dragging = $state<NoteMeta | null>(null);
  let dropOver = $state<string | null>(null);

  async function dropTo(folder: string | null) {
    const n = dragging;
    dragging = null;
    dropOver = null;
    if (!n || n.folder === folder) return;
    await setMeta(n, { folder });
    toast(t("nt.moved", { f: folder ? folders.find((f) => f.id === folder)?.name ?? "" : t("nt.noFolder") }));
  }

  /** Находит упомянутых участников: «@Имя» в тексте комментария. */
  function mentionsIn(text: string): string[] {
    return members.filter((m) => !m.me && m.name && text.toLocaleLowerCase().includes("@" + m.name.toLocaleLowerCase())).map((m) => m.device);
  }

  function addMention(m: SharedMember, target: "draft" | "reply") {
    const tag = "@" + m.name + " ";
    if (target === "draft" && draft) draft.text = (draft.text && !draft.text.endsWith(" ") ? draft.text + " " : draft.text) + tag;
    if (target === "reply") replyText = (replyText && !replyText.endsWith(" ") ? replyText + " " : replyText) + tag;
  }
  const isTrashed = $derived(current?.kind === "personal" && current.meta.trashed > 0);

  async function loadLists() {
    const [n, s, f, tp, r] = await Promise.all([
      api.notes(), api.sharedList().catch(() => shared), api.noteFolders().catch(() => []), api.templates().catch(() => []), api.reminders().catch(() => []),
    ]);
    notes = n; shared = s; folders = f; templates = tp; reminders = r;
  }

  async function closeShared() {
    if (provider) {
      const id = provider.id;
      provider.destroy();
      provider = null;
      commentsMap = null;
      comments = [];
      peers = [];
      draft = null;
      api.sharedClose(id).catch(() => {});
    }
  }

  async function select(c: Current | null) {
    await flush();
    await closeShared();
    current = c;
    doc = null;
    locked = false;
    unlockPw = "";
    unlockErr = "";
    tagInput = null;
    editorKey++;
    if (!c) return;
    if (c.kind === "personal") {
      const d = (await api.noteGet(c.meta.id)) as any;
      if (current?.kind !== "personal" || current.meta.id !== c.meta.id) return;
      if (d && d.secret === true && !d.type) locked = true;
      else doc = d;
      return;
    }
    opening = true;
    try {
      const o = await api.sharedOpen(c.meta.id);
      // current хранится как реактивная копия, поэтому сравниваем по id, а не по ссылке.
      if (current?.kind !== "shared" || current.meta.id !== c.meta.id) return;
      myRole = o.role;
      online = o.online;
      me = { name: o.name, color: colorFor(o.me || o.name) };
      const p = new ClavisProvider(c.meta.id, fromB64(o.state), me);
      p.awareness.on("change", () => (peers = p.peers()));
      api.sharedMembers(c.meta.id).then((m) => (members = m)).catch(() => (members = []));
      commentsMap = p.doc.getMap("comments");
      const sync = () => (comments = [...commentsMap!.values()].map((v) => v as CommentData).sort((a, b) => a.at - b.at));
      commentsMap.observe(sync);
      sync();
      provider = p;
    } catch (e) {
      toast(sharedError(e));
      current = null;
    } finally {
      opening = false;
    }
  }

  async function openTarget(tg: { kind: string; id: string }) {
    // Уведомление знает только id: ищем и среди личных, и среди общих заметок.
    const n = notes.find((x) => x.id === tg.id);
    if (n) { if (n.trashed) filter = "trash"; await select({ kind: "personal", meta: n }); return; }
    const s = shared.find((x) => x.id === tg.id);
    if (s) { filter = "all"; await select({ kind: "shared", meta: s }); }
  }

  onMount(() => {
    const offs: Array<() => void> = [];
    (async () => {
      await loadLists();
      const tg = nav.target;
      if (tg && (tg.kind === "note" || tg.kind === "shared")) {
        nav.target = null;
        await openTarget(tg);
      } else {
        const first = [...live].sort((a, b) => Number(b.pinned) - Number(a.pinned))[0];
        await select(first ? { kind: "personal", meta: first } : sharedNotes[0] ? { kind: "shared", meta: sharedNotes[0] } : null);
      }
    })();
    listen<boolean>("shared:online", (v) => (online = v)).then((o) => offs.push(o));
    listen("reminders:changed", async () => (reminders = await api.reminders().catch(() => reminders))).then((o) => offs.push(o));
    listen("notes:changed", async () => (notes = await api.notes())).then((o) => offs.push(o));
    listen<{ id: string }>("shared:removed", async (p) => {
      const s = shared.find((x) => x.id === p.id);
      if (s) toast(t("sh.removed", { t: s.title || t("lib.untitled") }), 4000);
      if (current?.kind === "shared" && current.meta.id === p.id) await select(null);
      shared = shared.filter((x) => x.id !== p.id);
    }).then((o) => offs.push(o));
    return () => offs.forEach((o) => o());
  });

  $effect(() => {
    const tg = nav.target;
    if (tg && (tg.kind === "note" || tg.kind === "shared") && notes.length + shared.length) {
      nav.target = null;
      openTarget(tg);
    }
  });

  onDestroy(() => {
    flush();
    closeShared();
  });

  function syncDeadlines(id: string, json: unknown, title: string) {
    const items = collectDeadlines(json, title || t("lib.untitled"));
    const key = JSON.stringify(items);
    if (lastDue.get(id) === key) return;
    if (!lastDue.has(id) && !items.length) { lastDue.set(id, key); return; }
    lastDue.set(id, key);
    api.remindersSet(id, items).then(async () => (reminders = await api.reminders())).catch(() => {});
  }

  async function flush() {
    ed?.flushChange();
    clearTimeout(timer);
    if (!pending || current?.kind !== "personal") return;
    const { doc: d, title } = pending;
    pending = null;
    doc = d;
    const meta = current.meta;
    try {
      const saved = await api.noteSave({ id: meta.id, title, doc: d, updated: meta.updated });
      if (current?.kind === "personal" && current.meta.id === saved.id) current = { kind: "personal", meta: saved };
      notes = notes.map((n) => (n.id === saved.id ? saved : n)).sort((a, b) => b.updated - a.updated);
      savedAt = Date.now();
      dirty = false;
      syncDeadlines(saved.id, d, title);
    } catch (e) {
      toast(t("toast.error", { e: String(e) }));
    }
  }

  function onchange(d: unknown, title: string) {
    if (current?.kind === "personal") {
      if (isTrashed) return;
      pending = { doc: d, title };
      dirty = true;
      clearTimeout(timer);
      timer = setTimeout(flush, 600);
    } else if (current?.kind === "shared") {
      const id = current.meta.id;
      clearTimeout(titleTimer);
      titleTimer = setTimeout(() => {
        if (current?.kind === "shared" && current.meta.title !== title) {
          current = { kind: "shared", meta: { ...current.meta, title, updated: Date.now() } };
          shared = shared.map((s) => (s.id === id ? { ...s, title, updated: Date.now() } : s));
          api.sharedTitle(id, title).catch(() => {});
        }
      }, 800);
      clearTimeout(dueTimer);
      dueTimer = setTimeout(() => syncDeadlines(id, d, title), 1500);
    }
  }

  // ——— создание ———

  async function createPersonal(content?: unknown, focus = true) {
    if (filter === "trash") filter = "all";
    const folder = filter !== "all" && filter !== "none" ? filter : null;
    const n = await api.noteSave({ id: "", title: "", doc: content ?? { type: "doc", content: [{ type: "heading", attrs: { level: 1 } }] }, updated: 0 }, folder);
    notes = [n, ...notes];
    await select({ kind: "personal", meta: n });
    if (content) {
      const title = (content as any)?.content?.[0]?.content?.[0]?.text ?? "";
      pending = { doc: content, title };
      await flush();
    }
    await tick();
    if (focus) ed?.focusStart();
  }

  function newMenu(e: MouseEvent) {
    const items: MenuItem[] = [
      { label: t("nt.blank"), icon: "note", hint: "Ctrl+N", run: () => createPersonal() },
      { header: t("nt.templates") },
      ...builtInTemplates().map((tp) => ({ label: tp.title, icon: "template", run: () => createPersonal(tp.doc()) })),
    ];
    if (templates.length) {
      items.push({ header: t("nt.myTemplates") });
      for (const tp of templates) items.push({ label: tp.title, icon: "template", run: () => createPersonal(tp.doc) });
      items.push({ label: t("nt.manageTemplates"), icon: "trash", run: () => manageTemplates(e) });
    }
    items.push({ sep: true }, { label: t("sh.new"), icon: "users", run: () => createShared() }, { label: t("sh.join"), icon: "link", run: () => { joinOpen = true; joinErr = ""; } });
    openMenu(e.currentTarget as HTMLElement, items);
  }

  function manageTemplates(e: MouseEvent) {
    openMenu(e.currentTarget as HTMLElement ?? e, [
      { header: t("nt.deleteTemplate") },
      ...templates.map((tp) => ({ label: tp.title, icon: "trash", danger: true, run: async () => {
        await api.templateDelete(tp.id);
        templates = templates.filter((x) => x.id !== tp.id);
        toast(t("nt.templateDeleted", { t: tp.title }), 4000, { label: t("common.undo"), run: async () => { const back = await api.templateSave(tp.title, tp.doc); templates = [...templates, back]; } });
      } })),
    ]);
  }

  async function createShared(title = "", content?: unknown) {
    try {
      const m = await api.sharedCreate(title, "note");
      shared = [m, ...shared];
      filter = "all";
      await select({ kind: "shared", meta: m });
      await tick();
      // Новая общая заметка начинается с заголовка, как и личная.
      ed?.setContent(content ?? { type: "doc", content: [{ type: "heading", attrs: { level: 1 } }] });
      if (!content) ed?.focusStart();
      return true;
    } catch (e) {
      toast(sharedError(e));
      return false;
    }
  }

  async function makeShared() {
    if (current?.kind !== "personal") return;
    if (current.meta.secret) return toast(t("nt.secretNoShare"), 3500);
    if (!(await ask({ title: t("sh.makeSharedQ"), text: t("sh.makeSharedConfirm"), ok: t("sh.makeShared") }))) return;
    await flush();
    const personal = current.meta;
    const content = doc;
    if (await createShared(personal.title, content)) {
      await api.noteDelete(personal.id);
      await api.notePurge(personal.id);
      notes = notes.filter((n) => n.id !== personal.id);
    }
  }

  async function join(e: Event) {
    e.preventDefault();
    joining = true;
    joinErr = "";
    try {
      const id = await api.sharedJoin(joinLink.trim());
      joinOpen = false;
      joinLink = "";
      shared = await api.sharedList();
      const m = shared.find((s) => s.id === id);
      if (m) { filter = "all"; await select({ kind: "shared", meta: m }); }
    } catch (err) {
      joinErr = sharedError(err);
    } finally {
      joining = false;
    }
  }

  // ——— удаление и корзина ———

  async function trashNote(n: NoteMeta) {
    if (current?.kind === "personal" && current.meta.id === n.id) { clearTimeout(timer); await flush(); }
    await api.noteDelete(n.id);
    lastDue.delete(n.id);
    const wasOpen = current?.kind === "personal" && current.meta.id === n.id;
    const idx = visibleNotes.findIndex((x) => x.id === n.id);
    notes = await api.notes();
    reminders = await api.reminders().catch(() => reminders);
    if (wasOpen) {
      const next = visibleNotes[Math.min(idx, visibleNotes.length - 1)];
      await select(next ? { kind: "personal", meta: next } : null);
    }
    toast(t("nt.trashed", { t: n.title || t("lib.untitled") }), 6000, {
      label: t("common.undo"),
      run: async () => {
        const back = await api.noteRestore(n.id);
        notes = await api.notes();
        await select({ kind: "personal", meta: back });
      },
    });
  }

  async function restoreNote(n: NoteMeta) {
    const back = await api.noteRestore(n.id);
    notes = await api.notes();
    toast(t("nt.restoredNote"));
    if (!trash.length) filter = "all";
    await select({ kind: "personal", meta: back });
  }

  async function purgeNote(n: NoteMeta) {
    if (!(await ask({ title: t("nt.purgeQ"), text: t("nt.purgeText"), ok: t("nt.purge"), danger: true }))) return;
    await api.notePurge(n.id);
    notes = notes.filter((x) => x.id !== n.id);
    if (current?.kind === "personal" && current.meta.id === n.id) await select(trash[0] ? { kind: "personal", meta: trash[0] } : null);
  }

  async function emptyTrash() {
    if (!(await ask({ title: t("nt.emptyTrashQ", { n: trash.length }), text: t("nt.purgeText"), ok: t("nt.emptyTrash"), danger: true }))) return;
    await api.trashEmpty();
    notes = notes.filter((n) => !n.trashed);
    filter = "all";
    await select(null);
  }

  // ——— папки, теги, закрепление ———

  async function setMeta(n: NoteMeta, patch: Partial<Pick<NoteMeta, "folder" | "tags" | "pinned">>) {
    const m = { folder: n.folder, tags: n.tags, pinned: n.pinned, ...patch };
    const saved = await api.noteMetaSet(n.id, m.folder, m.tags, m.pinned);
    notes = notes.map((x) => (x.id === saved.id ? { ...saved } : x));
    if (current?.kind === "personal" && current.meta.id === saved.id) current = { kind: "personal", meta: { ...current.meta, folder: saved.folder, tags: saved.tags, pinned: saved.pinned } };
    return saved;
  }

  async function togglePin(c: Current | null = current) {
    if (!c) return;
    if (c.kind === "personal") {
      const n = notes.find((x) => x.id === c.meta.id) ?? c.meta;
      const s = await setMeta(n, { pinned: !n.pinned });
      toast(s.pinned ? t("toast.pinned") : t("toast.unpinned"));
    } else {
      const pinned = !c.meta.pinned;
      await api.sharedPin(c.meta.id, pinned).catch(() => {});
      shared = shared.map((s) => (s.id === c.meta.id ? { ...s, pinned } : s));
      if (current?.kind === "shared" && current.meta.id === c.meta.id) current = { kind: "shared", meta: { ...current.meta, pinned } };
      toast(pinned ? t("toast.pinned") : t("toast.unpinned"));
    }
  }

  async function newFolder(moveNote?: NoteMeta) {
    const name = (await askText({ title: t("lib.newFolder"), ok: t("common.create"), placeholder: t("lib.folderName") }))?.trim();
    if (!name) return;
    const f = await api.noteFolderSave({ id: "", name });
    folders = [...folders, f];
    if (moveNote) await setMeta(moveNote, { folder: f.id });
    else filter = f.id;
  }

  async function renameFolder(f: Folder) {
    const name = (await askText({ title: t("nt.renameFolder"), ok: t("common.save"), value: f.name }))?.trim();
    if (!name || name === f.name) return;
    const saved = await api.noteFolderSave({ id: f.id, name });
    folders = folders.map((x) => (x.id === f.id ? saved : x));
  }

  async function deleteFolder(f: Folder) {
    if (!(await ask({ title: t("lib.deleteFolderQ", { n: f.name }), text: t("nt.deleteFolderText"), ok: t("lib.delete"), danger: true }))) return;
    await api.noteFolderDelete(f.id);
    folders = folders.filter((x) => x.id !== f.id);
    notes = await api.notes();
    if (filter === f.id) filter = "all";
  }

  function filterMenu(e: MouseEvent) {
    const count = (fn: (n: NoteMeta) => boolean) => String(live.filter(fn).length);
    const items: MenuItem[] = [
      { label: t("nt.all"), checked: filter === "all", hint: String(live.length + sharedNotes.length), run: () => (filter = "all") },
      ...folders.map((f) => ({ label: f.name, icon: "folder", checked: filter === f.id, hint: count((n) => n.folder === f.id), run: () => (filter = f.id) })),
      { label: t("nt.noFolder"), checked: filter === "none", hint: count((n) => !n.folder), run: () => (filter = "none") },
      { sep: true },
      { label: t("lib.newFolder"), icon: "plus", run: () => newFolder() },
    ];
    if (folders.length) items.push({ label: t("nt.manageFolders"), icon: "settings", run: () => foldersMenu(e) });
    items.push({ sep: true }, { label: t("nav.trash"), icon: "trash", checked: filter === "trash", hint: trash.length ? String(trash.length) : "", run: () => (filter = "trash") });
    openMenu(e.currentTarget as HTMLElement, items);
  }

  function foldersMenu(e: MouseEvent) {
    const items: MenuItem[] = [];
    for (const f of folders) {
      items.push({ header: f.name }, { label: t("nt.renameFolder"), icon: "text", run: () => renameFolder(f) }, { label: t("lib.delete"), icon: "trash", danger: true, run: () => deleteFolder(f) });
    }
    openMenu(e.currentTarget as HTMLElement, items);
  }

  function moveMenu(anchor: HTMLElement | MouseEvent, n: NoteMeta) {
    openMenu(anchor, [
      { header: t("nt.moveTo") },
      ...folders.map((f) => ({ label: f.name, icon: "folder", checked: n.folder === f.id, run: () => setMeta(n, { folder: f.id }) })),
      { label: t("nt.noFolder"), checked: !n.folder, run: () => setMeta(n, { folder: null }) },
      { sep: true },
      { label: t("lib.newFolder"), icon: "plus", run: () => newFolder(n) },
    ]);
  }

  function itemMenu(e: MouseEvent, c: Current) {
    if (c.kind === "personal") {
      const n = c.meta;
      if (n.trashed) {
        openMenu(e, [
          { label: t("nt.restoreNote"), icon: "refresh", run: () => restoreNote(n) },
          { label: t("nt.purge"), icon: "trash", danger: true, run: () => purgeNote(n) },
        ]);
        return;
      }
      openMenu(e, [
        { label: n.pinned ? t("nt.unpin") : t("nt.pin"), icon: "pin", run: () => togglePin(c) },
        { label: t("nt.moveTo") + "…", icon: "folder", run: () => moveMenu(e, n) },
        { sep: true },
        { label: t("nt.toTrash"), icon: "trash", danger: true, hint: "Del", run: () => trashNote(n) },
      ]);
    } else {
      openMenu(e, [
        { label: c.meta.pinned ? t("nt.unpin") : t("nt.pin"), icon: "pin", run: () => togglePin(c) },
        { label: c.meta.role === "owner" ? t("sh.invite") : t("sh.members"), icon: "users", run: async () => { await select(c); shareOpen = true; } },
      ]);
    }
  }

  async function addTag() {
    const n = current?.kind === "personal" ? notes.find((x) => x.id === current!.meta.id) : null;
    const raw = (tagInput ?? "").trim().replace(/^#/, "");
    tagInput = null;
    if (!n || !raw) return;
    const tags = [...new Set([...n.tags, ...raw.split(/[,\s]+/).filter(Boolean)])];
    await setMeta(n, { tags });
  }

  async function removeTag(tag: string) {
    const n = current?.kind === "personal" ? notes.find((x) => x.id === current!.meta.id) : null;
    if (n) await setMeta(n, { tags: n.tags.filter((x) => x !== tag) });
  }

  // ——— секретные заметки ———

  async function unlock(e: Event) {
    e.preventDefault();
    if (current?.kind !== "personal" || !unlockPw) return;
    try {
      doc = await api.noteUnlock(current.meta.id, unlockPw);
      locked = false;
      unlockPw = "";
      unlockErr = "";
      editorKey++;
    } catch (err) {
      unlockErr = String(err) === "wrong_password" ? t("nt.wrongPassword") : t("toast.error", { e: String(err) });
    }
  }

  async function setSecret(e: Event) {
    e.preventDefault();
    if (current?.kind !== "personal") return;
    if (pw1.length < 4) return (pwErr = t("nt.passwordShort"));
    if (pw1 !== pw2) return (pwErr = t("nt.passwordMismatch"));
    await flush();
    try {
      const m = await api.noteSetSecret(current.meta.id, pw1, ed?.getJSON() ?? doc);
      current = { kind: "personal", meta: m };
      notes = notes.map((n) => (n.id === m.id ? m : n));
      secretOpen = null;
      pw1 = pw2 = pwErr = "";
      toast(t("nt.secretOn"));
    } catch (err) {
      pwErr = String(err) === "password_short" ? t("nt.passwordShort") : t("toast.error", { e: String(err) });
    }
  }

  async function removeSecret() {
    if (current?.kind !== "personal") return;
    if (!(await ask({ title: t("nt.removeSecretQ"), text: t("nt.removeSecretText"), ok: t("nt.removeSecret") }))) return;
    await flush();
    const m = await api.noteSetSecret(current.meta.id, null, ed?.getJSON() ?? doc);
    current = { kind: "personal", meta: m };
    notes = notes.map((n) => (n.id === m.id ? m : n));
    toast(t("nt.secretOff"));
  }

  async function lockNow() {
    if (current?.kind !== "personal") return;
    await flush();
    await api.noteLockSecret(current.meta.id);
    doc = null;
    locked = true;
  }

  // ——— версии ———

  async function openVersions() {
    if (!current) return;
    await flush();
    versions = await api.versions(current.meta.id).catch(() => []);
    vSel = null;
    versionsOpen = true;
    if (versions[0]) pickVersion(versions[0]);
  }

  async function pickVersion(v: VersionMeta) {
    const r = await api.versionGet(v.id).catch(() => null);
    if (!r) return;
    const json = r.kind === "json" ? r.doc : ed?.yjsToJSON(fromB64(r.state));
    vSel = { id: v.id, doc: json };
  }

  async function restoreVersion() {
    if (!vSel || !current) return;
    const json = vSel.doc;
    versionsOpen = false;
    if (current.kind === "personal") {
      const title = (json as any)?.content?.[0]?.content?.map((c: any) => c.text ?? "").join("") ?? current.meta.title;
      pending = { doc: json, title };
      await flush();
      editorKey++;
    } else {
      ed?.setContent(json);
    }
    toast(t("nt.restored"));
  }

  // ——— экспорт ———

  const fileName = () => (current?.meta.title || t("lib.untitled")).replace(/[\\/:*?"<>|]+/g, " ").trim().slice(0, 120) || "note";

  async function resolvedJSON(): Promise<unknown> {
    const json = lowerCustom(structuredClone($state.snapshot(current?.kind === "personal" ? (ed?.getJSON() ?? doc) : ed?.getJSON())));
    if (current?.kind === "shared") {
      const id = current.meta.id;
      const walk = async (n: any) => {
        if (n?.type === "image" && typeof n.attrs?.src === "string" && n.attrs.src.startsWith("clavis-img:"))
          n.attrs.src = await api.sharedImageGet(id, n.attrs.src.slice("clavis-img:".length)).catch(() => "");
        for (const c of n?.content ?? []) await walk(c);
      };
      await walk(json);
    }
    return json;
  }

  async function exportAs(kind: "docx" | "pdf" | "md") {
    if (!current || busy) return;
    await flush();
    busy = true;
    try {
      const json = await resolvedJSON();
      let path: string | null = null;
      if (kind === "docx") {
        const bytes = await toDocx(json, current.meta.title || t("lib.untitled"));
        let bin = "";
        for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        path = await api.noteExportDocx(fileName(), btoa(bin));
      } else if (kind === "md") {
        path = await api.exportText(fileName(), "md", toMarkdown(json));
      } else {
        path = await api.exportPdf(fileName(), docToHtml(json, formatDeadline));
      }
      if (path) toast(t("toast.exported", { p: path }), 3500);
    } catch (e) {
      toast(t("toast.error", { e: String(e) }));
    } finally {
      busy = false;
    }
  }

  async function saveAsTemplate() {
    if (!current) return;
    await flush();
    const title = (await askText({ title: t("nt.saveTemplate"), ok: t("common.save"), value: current.meta.title, placeholder: t("nt.templateName") }))?.trim();
    if (!title) return;
    const json = await resolvedJSON();
    const tp = await api.templateSave(title, json);
    templates = [...templates, tp];
    toast(t("nt.templateSaved", { t: title }));
  }

  function moreMenu(e: MouseEvent) {
    if (!current) return;
    const items: MenuItem[] = [
      { header: t("nt.export") },
      { label: t("nt.exportWord"), icon: "download", run: () => exportAs("docx") },
      { label: t("nt.exportPdf"), icon: "download", run: () => exportAs("pdf") },
      { label: t("nt.exportMd"), icon: "download", run: () => exportAs("md") },
      { sep: true },
    ];
    if (current.kind === "personal") {
      const n = current.meta;
      if (!n.secret) items.push({ label: t("nt.versions"), icon: "clock", run: openVersions });
      items.push({ label: t("nt.saveTemplate"), icon: "template", run: saveAsTemplate });
      items.push({ label: t("nt.moveTo") + "…", icon: "folder", run: () => moveMenu(e, notes.find((x) => x.id === n.id) ?? n) });
      if (n.secret) {
        items.push({ label: t("nt.lockNow"), icon: "lock", run: lockNow }, { label: t("nt.removeSecret"), icon: "shield", run: removeSecret });
      } else {
        items.push({ label: t("nt.makeSecret"), icon: "shield", run: () => { secretOpen = "set"; pw1 = pw2 = pwErr = ""; } });
        items.push({ label: t("sh.makeShared"), icon: "users", run: makeShared });
      }
      items.push({ sep: true }, { label: t("nt.toTrash"), icon: "trash", danger: true, run: () => trashNote(notes.find((x) => x.id === n.id) ?? n) });
    } else {
      items.push({ label: t("nt.versions"), icon: "clock", run: openVersions });
      items.push({ label: t("nt.saveTemplate"), icon: "template", run: saveAsTemplate });
      items.push({ sep: true }, { label: myRole === "owner" ? t("sh.delete") + "…" : t("sh.leave") + "…", icon: "trash", danger: true, run: () => (shareOpen = true) });
    }
    openMenu(e.currentTarget as HTMLElement, items);
  }

  // ——— комментарии ———

  function onaddcomment(id: string, quote: string) {
    commentsOpen = true;
    draft = { id, quote, text: "" };
    tick().then(() => document.querySelector<HTMLTextAreaElement>(".cdraft textarea")?.focus());
  }

  function cancelDraft() {
    if (draft) ed?.removeComment(draft.id);
    draft = null;
  }

  function saveDraft() {
    if (!draft || !commentsMap) return;
    const text = draft.text.trim();
    if (!text) return cancelDraft();
    const c: CommentData = { id: draft.id, author: me.name, color: me.color, text, quote: draft.quote, at: Date.now(), resolved: false, replies: [], mentions: mentionsIn(text) };
    commentsMap.set(c.id, c);
    draft = null;
  }

  function reply(c: CommentData) {
    const text = replyText.trim();
    if (!text || !commentsMap) return;
    commentsMap.set(c.id, { ...c, replies: [...c.replies, { author: me.name, color: me.color, text, at: Date.now(), mentions: mentionsIn(text) }] });
    replyText = "";
    replyTo = null;
  }

  function resolve(c: CommentData) {
    commentsMap?.set(c.id, { ...c, resolved: true });
    ed?.removeComment(c.id);
  }

  async function deleteComment(c: CommentData) {
    if (!(await ask({ title: t("nt.deleteCommentQ"), ok: t("lib.delete"), danger: true }))) return;
    commentsMap?.delete(c.id);
    ed?.removeComment(c.id);
  }

  async function oncommentclick(id: string) {
    commentsOpen = true;
    flashed = id;
    await tick();
    document.querySelector(`[data-cid="${id}"]`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    setTimeout(() => { if (flashed === id) flashed = null; }, 1200);
  }

  // ——— клавиатура ———

  function onkey(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.code === "KeyN" && !e.shiftKey) { e.preventDefault(); createPersonal(); return; }
    const inList = (e.target as HTMLElement)?.closest?.(".items");
    if (inList && e.key === "Delete" && current?.kind === "personal") {
      e.preventDefault();
      if (current.meta.trashed) purgeNote(current.meta);
      else trashNote(current.meta);
    }
  }

  const isOn = (kind: string, id: string) => current?.kind === kind && current.meta.id === id;
  const initials = (name: string) => name.trim().split(/\s+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? "").join("") || "?";
  const curNote = $derived(current?.kind === "personal" ? notes.find((n) => n.id === current!.meta.id) ?? current.meta : null);
  const folderName = (id: string | null) => (id ? folders.find((f) => f.id === id)?.name ?? t("nt.noFolder") : t("nt.noFolder"));
  const noteOf = (r: Reminder) => live.find((n) => n.id === r.note)?.title || sharedNotes.find((s) => s.id === r.note)?.title || t("lib.untitled");
  const vTime = (at: number) => new Date(at).toLocaleString(i18n.lang === "ru" ? "ru-RU" : "en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
</script>

<svelte:window onkeydown={onkey} />

<div class="split" class:withside={commentsOpen && current?.kind === "shared"}>
  <section class="col-list">
    <div class="listhead">
      <div class="searchbox"><Icon name="search" /><input bind:value={query} placeholder={t("nt.searchPh")} spellcheck="false" /></div>
      <button class="btn sq" onclick={newMenu} title="{t('lib.newNote')}  Ctrl+N" aria-label={t("lib.newNote")}><Icon name="plus" /></button>
    </div>
    <div class="filterbar">
      <button class="filter" onclick={filterMenu} aria-haspopup="menu">
        <Icon name={filter === "trash" ? "trash" : "folder"} size={13} /><span>{filterLabel}</span><svg viewBox="0 0 10 10"><path d="M2.5 4l2.5 2.5L7.5 4" /></svg>
      </button>
      {#if filter === "trash" && trash.length}
        <button class="linkbtn danger" onclick={emptyTrash}>{t("nt.emptyTrash")}</button>
      {:else if filter !== "all"}
        <button class="linkbtn" onclick={() => (filter = "all")}>{t("nt.showAll")}</button>
      {/if}
    </div>

    {#if dragging}
      <div class="dropzone" transition:slide={{ duration: 120 }}>
        <span class="dz-h">{t("nt.dropToFolder")}</span>
        {#each [...folders.map((f) => ({ id: f.id as string | null, name: f.name })), { id: null, name: t("nt.noFolder") }] as f (f.id ?? "none")}
          <div class="dz" class:over={dropOver === (f.id ?? "none")} role="button" tabindex="-1"
            ondragover={(e) => { e.preventDefault(); dropOver = f.id ?? "none"; }}
            ondragleave={() => { if (dropOver === (f.id ?? "none")) dropOver = null; }}
            ondrop={(e) => { e.preventDefault(); dropTo(f.id); }}><Icon name="folder" size={12} />{f.name}</div>
        {/each}
      </div>
    {/if}
    <div class="items">
      {#if upcoming.length && filter !== "trash" && !query}
        <div class="sect">{t("nt.deadlines")}</div>
        {#each upcoming as r (r.id)}
          <button class="due" class:over={r.at < Date.now()} onclick={() => openTarget({ kind: live.some((n) => n.id === r.note) ? "note" : "shared", id: r.note })}>
            <span class="dot"></span>
            <span class="dtext"><span>{r.text}</span><small>{noteOf(r)}</small></span>
            <span class="dwhen">{r.at < Date.now() ? t("nt.overdue") : formatDeadline(r.at)}</span>
          </button>
        {/each}
      {/if}

      {#snippet noteItem(n: NoteMeta)}
        <div class="item" class:on={isOn("personal", n.id)} role="button" tabindex="0"
          draggable={!n.trashed}
          ondragstart={(e) => { dragging = n; e.dataTransfer?.setData("text/plain", n.title); if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
          ondragend={() => { dragging = null; dropOver = null; }}
          onclick={() => select({ kind: "personal", meta: n })}
          onkeydown={(e) => { if (e.key === "Enter") select({ kind: "personal", meta: n }); }}
          oncontextmenu={(e) => itemMenu(e, { kind: "personal", meta: n })}>
          <span class="it-title">
            {#if n.secret}<Icon name="lock" size={12} />{/if}
            <span>{n.title || t("lib.untitled")}</span>
          </span>
          <span class="it-sub">
            {n.trashed ? t("nt.deletedAgo", { t: ago(n.trashed) }) : ago(n.updated)}{#if filter === "all" && n.folder}<span class="fold">{folderName(n.folder)}</span>{/if}{#each n.tags.slice(0, 3) as tag}<span class="tg">#{tag}</span>{/each}
          </span>
          <span class="hover">
            {#if n.trashed}
              <button title={t("nt.restoreNote")} aria-label={t("nt.restoreNote")} onclick={(e) => { e.stopPropagation(); restoreNote(n); }}><Icon name="refresh" size={13} /></button>
              <button title={t("nt.purge")} aria-label={t("nt.purge")} onclick={(e) => { e.stopPropagation(); purgeNote(n); }}><Icon name="close" size={13} /></button>
            {:else}
              <button class:act={n.pinned} title={n.pinned ? t("nt.unpin") : t("nt.pin")} aria-label={n.pinned ? t("nt.unpin") : t("nt.pin")} onclick={(e) => { e.stopPropagation(); togglePin({ kind: "personal", meta: n }); }}><Icon name="pin" size={13} /></button>
              <button title={t("nt.toTrash")} aria-label={t("nt.toTrash")} onclick={(e) => { e.stopPropagation(); trashNote(n); }}><Icon name="trash" size={13} /></button>
            {/if}
          </span>
          {#if n.pinned && !n.trashed}<span class="pinned"><Icon name="pin" size={11} /></span>{/if}
        </div>
      {/snippet}

      {#if pinnedNotes.length}
        <div class="sect">{t("nt.pinned")}</div>
        {#each pinnedNotes as n (n.id)}{@render noteItem(n)}{/each}
      {/if}
      {#if filter === "trash"}
        <p class="hintline">{t("nt.trashHelp")}</p>
      {:else if otherNotes.length || (!query && !pinnedNotes.length)}
        <div class="sect">{t("sh.mine")}</div>
      {/if}
      {#each otherNotes as n (n.id)}{@render noteItem(n)}{/each}
      {#if !visibleNotes.length && filter !== "all"}
        <p class="hintline">{query ? t("picker.empty.search", { q: query }) : filter === "trash" ? t("nt.trashEmpty") : t("nt.folderEmpty")}</p>
      {:else if !visibleNotes.length && !query}
        <button class="empty-shared" onclick={() => createPersonal()}><Icon name="note" /><span>{t("lib.newNote")}</span></button>
      {/if}

      {#if filter === "all"}
        <div class="sect row">
          <span>{t("sh.shared")}</span>
          <span class="sect-actions">
            <button class="mini" onclick={() => createShared()} title={t("sh.new")} aria-label={t("sh.new")}><Icon name="plus" size={13} /></button>
            <button class="mini" onclick={() => { joinOpen = true; joinErr = ""; }} title={t("sh.join")} aria-label={t("sh.join")}><Icon name="link" size={13} /></button>
          </span>
        </div>
        {#each visibleShared as s (s.id)}
          <div class="item" class:on={isOn("shared", s.id)} role="button" tabindex="0"
            onclick={() => select({ kind: "shared", meta: s })}
            onkeydown={(e) => { if (e.key === "Enter") select({ kind: "shared", meta: s }); }}
            oncontextmenu={(e) => itemMenu(e, { kind: "shared", meta: s })}>
            <span class="it-title"><Icon name="users" size={12} /><span>{s.title || t("lib.untitled")}</span></span>
            <span class="it-sub">{ago(s.updated)}</span>
            <span class="hover">
              <button class:act={s.pinned} title={s.pinned ? t("nt.unpin") : t("nt.pin")} aria-label={s.pinned ? t("nt.unpin") : t("nt.pin")} onclick={(e) => { e.stopPropagation(); togglePin({ kind: "shared", meta: s }); }}><Icon name="pin" size={13} /></button>
            </span>
            {#if s.pinned}<span class="pinned"><Icon name="pin" size={11} /></span>{/if}
          </div>
        {:else}
          {#if !query}
            <button class="empty-shared" onclick={() => createShared()}><Icon name="users" /><span>{t("sh.new")}</span></button>
          {/if}
        {/each}
      {/if}
    </div>
  </section>

  <section class="col-edit">
    {#if current}
      <div class="docbar">
        {#if current.kind === "shared"}
          <span class="status" class:off={!online} title={online ? t("sh.synced") : t("sh.offline")}>
            <i></i><span class="st-text">{online ? t("sh.synced") : t("sh.offline")}</span>
          </span>
          {#if peers.length}
            <span class="peers" title={peers.map((p) => p.name).join(", ") + " — " + t("sh.here")}>
              {#each peers.slice(0, 5) as p (p.clientId)}
                <span class="av" style="background:{p.color}" in:fade={{ duration: 150 }}>{initials(p.name)}</span>
              {/each}
              {#if peers.length > 5}<span class="av more">+{peers.length - 5}</span>{/if}
            </span>
          {/if}
        {:else if curNote && !isTrashed && !locked}
          <button class="chip" onclick={(e) => moveMenu(e.currentTarget, curNote)} title={t("nt.moveTo")}><Icon name="folder" size={12} />{folderName(curNote.folder)}</button>
          <span class="tags">
            {#each curNote.tags as tag (tag)}
              <span class="tag" transition:fade={{ duration: 100 }}>#{tag}<button aria-label="{t('lib.delete')} #{tag}" onclick={() => removeTag(tag)}><Icon name="close" size={10} /></button></span>
            {/each}
            {#if tagInput !== null}
              <!-- svelte-ignore a11y_autofocus -->
              <input class="taginput" list="note-tags" bind:value={tagInput} autofocus placeholder={t("nt.addTag")} onblur={addTag} onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); addTag(); } if (e.key === "Escape") tagInput = null; }} />
              <datalist id="note-tags">{#each allTags.filter((x) => !curNote.tags.includes(x)) as tg}<option value={tg}></option>{/each}</datalist>
            {:else}
              <button class="chip ghost" onclick={() => (tagInput = "")}><Icon name="tag" size={12} />{curNote.tags.length ? "" : t("nt.addTag")}</button>
            {/if}
          </span>
          <span class="saved faint">{dirty ? t("nt.saving") : savedAt ? t("lib.saved") : ago(curNote.updated)}</span>
        {/if}
        <span class="right">
          {#if !isTrashed && !locked}
            <button class="btn ghost" class:on={current.kind === "personal" ? curNote?.pinned : current.meta.pinned} onclick={() => togglePin()} title={(current.kind === "personal" ? curNote?.pinned : current.meta.pinned) ? t("nt.unpin") : t("nt.pin")} aria-label={t("nt.pin")}><Icon name="pin" /></button>
            {#if current.kind === "shared"}
              <button class="btn ghost" class:on={commentsOpen} onclick={() => (commentsOpen = !commentsOpen)} title={t("nt.comments")}><Icon name="comment" /><span class="bl">{t("nt.comments")}</span>{#if openComments.length}<span class="count">{openComments.length}</span>{/if}</button>
              <button class="btn ghost" onclick={() => (shareOpen = true)}><Icon name="users" /><span class="bl">{myRole === "owner" ? t("sh.invite") : t("sh.members")}</span></button>
            {:else if !curNote?.secret}
              <button class="btn ghost" onclick={makeShared} title={t("sh.makeShared")}><Icon name="users" /><span class="bl">{t("sh.makeShared")}</span></button>
            {/if}
            <button class="btn ghost" onclick={moreMenu} disabled={busy} title={t("nt.more")} aria-label={t("nt.more")} aria-haspopup="menu"><Icon name="more" /></button>
          {/if}
        </span>
      </div>

      {#if isTrashed && curNote}
        <div class="banner" transition:slide={{ duration: 140 }}>
          <span>{t("nt.inTrash")}</span>
          <button class="btn" onclick={() => restoreNote(curNote)}><Icon name="refresh" />{t("nt.restoreNote")}</button>
          <button class="btn ghost danger" onclick={() => purgeNote(curNote)}>{t("nt.purge")}</button>
        </div>
      {/if}

      {#if current.kind === "personal" && locked}
        <form class="locked" onsubmit={unlock} in:fade={{ duration: 140 }}>
          <span class="lk"><Icon name="lock" size={22} /></span>
          <h2>{current.meta.title || t("lib.untitled")}</h2>
          <p class="muted">{t("nt.lockedTitle")}</p>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="field" type="password" bind:value={unlockPw} placeholder={t("nt.password")} autocomplete="off" autofocus />
          {#if unlockErr}<p class="err">{unlockErr}</p>{/if}
          <button class="btn primary" disabled={!unlockPw}>{t("nt.unlock")}</button>
        </form>
      {:else if current.kind === "personal" && doc}
        {#key editorKey}
          <Editor bind:this={ed} {doc} {onchange} readonly={isTrashed} links={linkTargets} onopenlink={openLink} />
        {/key}
      {:else if current.kind === "shared" && provider}
        {#key provider}
          <Editor bind:this={ed} {onchange} collab={{ provider, docId: current.meta.id }} {onaddcomment} {oncommentclick} links={linkTargets} onopenlink={openLink} />
        {/key}
      {:else if opening}
        <div class="loading" in:fade={{ delay: 150, duration: 150 }}><span></span><span></span><span></span></div>
      {/if}
    {:else}
      <div class="empty">
        <p>{filter === "trash" ? t("nt.trashEmpty") : t("lib.selectNote")}</p>
        {#if filter !== "trash"}<button class="btn" onclick={() => createPersonal()}><Icon name="plus" />{t("lib.newNote")}</button>{/if}
      </div>
    {/if}
  </section>

  {#if commentsOpen && current?.kind === "shared"}
    <aside class="side" transition:fade={{ duration: 120 }}>
      <header class="sidehead">
        <span>{t("nt.comments")}</span>
        <button class="mini" onclick={() => (commentsOpen = false)} aria-label={t("win.close")}><Icon name="close" size={13} /></button>
      </header>
      {#snippet mentionChips(target: "draft" | "reply")}
        {#if members.some((m) => !m.me && m.name)}
          <div class="mchips">
            {#each members.filter((m) => !m.me && m.name) as m (m.device)}
              <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => addMention(m, target)}>@{m.name}</button>
            {/each}
          </div>
        {/if}
      {/snippet}
      <div class="clist">
        {#if draft}
          <div class="ccard cdraft" in:slide={{ duration: 140 }}>
            <p class="quote">{draft.quote}</p>
            <textarea class="field" rows="3" bind:value={draft.text} placeholder={t("nt.commentPh")}
              onkeydown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); saveDraft(); } if (e.key === "Escape") { e.preventDefault(); cancelDraft(); } }}></textarea>
            {@render mentionChips("draft")}
            <div class="cacts">
              <button class="btn ghost" onclick={cancelDraft}>{t("common.cancel")}</button>
              <button class="btn primary" disabled={!draft.text.trim()} onclick={saveDraft}>{t("nt.comment")}</button>
            </div>
          </div>
        {/if}
        {#each openComments as c (c.id)}
          <div class="ccard" class:flash={flashed === c.id} data-cid={c.id} transition:slide={{ duration: 140 }}>
            <button class="quote" onclick={() => { if (!ed?.focusComment(c.id)) toast(t("nt.quoteGone")); }}>{c.quote}</button>
            <div class="cmsg">
              <span class="av sm" style="background:{c.color}">{initials(c.author)}</span>
              <div><b>{c.author}</b> <span class="faint">{ago(c.at)}</span><p>{c.text}</p></div>
            </div>
            {#each c.replies as r}
              <div class="cmsg reply">
                <span class="av sm" style="background:{r.color}">{initials(r.author)}</span>
                <div><b>{r.author}</b> <span class="faint">{ago(r.at)}</span><p>{r.text}</p></div>
              </div>
            {/each}
            {#if replyTo === c.id}
              <!-- svelte-ignore a11y_autofocus -->
              <textarea class="field" rows="2" bind:value={replyText} autofocus placeholder={t("nt.reply")}
                onkeydown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); reply(c); } if (e.key === "Escape") { replyTo = null; } }}></textarea>
              {@render mentionChips("reply")}
              <div class="cacts">
                <button class="btn ghost" onclick={() => (replyTo = null)}>{t("common.cancel")}</button>
                <button class="btn primary" disabled={!replyText.trim()} onclick={() => reply(c)}>{t("nt.reply")}</button>
              </div>
            {:else}
              <div class="cacts left">
                <button class="linkbtn" onclick={() => { replyTo = c.id; replyText = ""; }}>{t("nt.reply")}</button>
                <button class="linkbtn" onclick={() => resolve(c)}><Icon name="check" size={12} />{t("nt.resolve")}</button>
                {#if c.author === me.name || myRole === "owner"}<button class="linkbtn danger" onclick={() => deleteComment(c)}>{t("lib.delete")}</button>{/if}
              </div>
            {/if}
          </div>
        {:else}
          {#if !draft}<p class="hintline">{t("nt.noComments")}</p>{/if}
        {/each}
        {#if resolvedComments.length}
          <button class="linkbtn resolvedtoggle" onclick={() => (showResolved = !showResolved)}>{t("nt.resolved")} · {resolvedComments.length}</button>
          {#if showResolved}
            {#each resolvedComments as c (c.id)}
              <div class="ccard done" transition:slide={{ duration: 120 }}>
                <p class="quote">{c.quote}</p>
                <p class="ctext"><b>{c.author}:</b> {c.text}</p>
                <div class="cacts left">
                  <button class="linkbtn danger" onclick={() => deleteComment(c)}>{t("lib.delete")}</button>
                </div>
              </div>
            {/each}
          {/if}
        {/if}
      </div>
    </aside>
  {/if}
</div>

{#if joinOpen}
  <Modal title={t("sh.joinTitle")} onclose={() => (joinOpen = false)}>
    <form class="dlg" onsubmit={join}>
      <p class="muted">{t("sh.joinHelp")}</p>
      <input class="field mono" bind:value={joinLink} placeholder="clavis:join/…" spellcheck="false" autocomplete="off" />
      {#if joinErr}<p class="err">{joinErr}</p>{/if}
      <div class="dlg-actions">
        <button type="button" class="btn ghost" onclick={() => (joinOpen = false)}>{t("common.cancel")}</button>
        <button class="btn primary" disabled={!joinLink.trim() || joining}>{t("sh.joinGo")}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if shareOpen && current?.kind === "shared"}
  <ShareModal id={current.meta.id} role={myRole} onclose={() => (shareOpen = false)} onleft={async () => {
    const id = current?.meta.id;
    shareOpen = false;
    await closeShared();
    shared = shared.filter((s) => s.id !== id);
    current = null;
    await select(live[0] ? { kind: "personal", meta: live[0] } : null);
  }} />
{/if}

{#if secretOpen && current?.kind === "personal"}
  <Modal title={t("nt.makeSecret")} onclose={() => (secretOpen = null)}>
    <form class="dlg" onsubmit={setSecret}>
      <p class="muted">{t("nt.secretHelp")}</p>
      <input class="field" type="password" bind:value={pw1} placeholder={t("nt.password")} autocomplete="new-password" />
      <input class="field" type="password" bind:value={pw2} placeholder={t("nt.password2")} autocomplete="new-password" />
      {#if pwErr}<p class="err">{pwErr}</p>{/if}
      <div class="dlg-actions">
        <button type="button" class="btn ghost" onclick={() => (secretOpen = null)}>{t("common.cancel")}</button>
        <button class="btn primary" disabled={!pw1 || !pw2}>{t("nt.makeSecret")}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if versionsOpen && current}
  <Modal title={t("nt.versions")} onclose={() => (versionsOpen = false)} width={820}>
    {#if !versions.length}
      <p class="muted vnone">{t("nt.noVersions")}</p>
    {:else}
      <div class="vgrid">
        <ul class="vlist">
          {#each versions as v (v.id)}
            <li><button class:on={vSel?.id === v.id} onclick={() => pickVersion(v)}><span>{vTime(v.at)}</span><small>{ago(v.at)}</small></button></li>
          {/each}
        </ul>
        <div class="vprev">
          {#if vSel}
            <div class="vdoc">{@html docToHtml(vSel.doc, formatDeadline)}</div>
            <div class="dlg-actions"><button class="btn primary" onclick={restoreVersion}><Icon name="refresh" />{t("nt.restore")}</button></div>
          {/if}
        </div>
      </div>
    {/if}
  </Modal>
{/if}

<style>
  .split { display: grid; grid-template-columns: 270px minmax(0, 1fr); height: 100%; min-height: 0; }
  .split.withside { grid-template-columns: 270px minmax(0, 1fr) 290px; }
  .col-list { border-right: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; }
  .listhead { display: flex; gap: 8px; padding: 12px 12px 8px; }
  .sq { padding: 0 8px; }
  .searchbox { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; padding: 0 8px; border: 1px solid var(--line); border-radius: var(--r-s); color: var(--faint); background: var(--sunk); }
  .searchbox input { flex: 1; min-width: 0; background: none; border: 0; height: 28px; user-select: text; }
  .filterbar { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 0 12px 8px; border-bottom: 1px solid var(--line); min-width: 0; }
  .filter { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 8px; margin-left: -8px; border-radius: var(--r-s); color: var(--muted); font-size: var(--t-s); min-width: 0; }
  .filter span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .filter:hover { color: var(--text); background: var(--raise); }
  .filter svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-width: 1.4; flex: none; }
  .linkbtn { display: inline-flex; align-items: center; gap: 4px; font-size: var(--t-xs); color: var(--faint); padding: 3px 4px; border-radius: var(--r-s); white-space: nowrap; }
  .linkbtn:hover { color: var(--text); }
  .linkbtn.danger:hover { color: var(--danger); }
  .items { overflow-y: auto; padding: 4px 6px 12px; flex: 1; min-height: 0; }
  .sect { padding: 12px 10px 4px; font-size: var(--t-xs); color: var(--faint); }
  .sect.row { display: flex; align-items: center; justify-content: space-between; margin-top: 8px; }
  .sect-actions { display: flex; gap: 2px; }
  .mini { color: var(--faint); padding: 4px; border-radius: var(--r-s); display: grid; }
  .mini:hover { color: var(--text); background: var(--raise); }
  .hintline { color: var(--faint); font-size: var(--t-s); line-height: 1.5; padding: 8px 10px; margin: 0; }

  .item { position: relative; width: 100%; min-width: 0; display: grid; grid-template-columns: minmax(0, 1fr); gap: 1px; text-align: left; padding: 8px 10px; border-radius: var(--r-s); transition: background 90ms var(--ease); cursor: default; content-visibility: auto; contain-intrinsic-size: auto 56px; }
  .item:hover { background: var(--raise); }
  .item:focus-visible { outline: 1px solid var(--muted); outline-offset: -1px; }
  .item.on { background: var(--text); color: var(--bg); }
  .item.on .it-sub, .item.on .it-title :global(svg), .item.on .pinned { color: var(--on-text-muted); }
  .it-title { display: flex; align-items: center; gap: 6px; min-width: 0; padding-right: 14px; }
  .it-title > span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .it-title :global(svg) { color: var(--faint); flex: none; }
  .it-sub { font-size: var(--t-xs); color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .fold, .tg { margin-left: 8px; }
  .pinned { position: absolute; top: 9px; right: 10px; color: var(--faint); display: grid; }
  .hover { position: absolute; right: 6px; top: 50%; translate: 0 -50%; display: none; gap: 1px; padding: 2px; border-radius: var(--r-s); background: var(--raise); box-shadow: -8px 0 8px var(--raise); }
  .item.on .hover { background: var(--text); box-shadow: -8px 0 8px var(--text); }
  .item:hover .hover, .item:focus-within .hover { display: flex; }
  .item:hover .pinned { display: none; }
  .hover button { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r-s); color: var(--muted); }
  .hover button:hover { color: var(--text); background: var(--line); }
  .item.on .hover button { color: var(--on-text-muted); }
  .item.on .hover button:hover { color: var(--bg); background: rgba(21, 21, 20, 0.1); }
  .hover button.act { color: var(--text); }
  .empty-shared { width: 100%; display: flex; align-items: center; gap: 8px; padding: 10px; margin-top: 2px; border: 1px dashed var(--line-strong); border-radius: var(--r-s); color: var(--muted); font-size: var(--t-s); }
  .empty-shared:hover { color: var(--text); border-color: var(--muted); }

  .due { width: 100%; display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: var(--r-s); text-align: left; min-width: 0; }
  .due:hover { background: var(--raise); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--ok); flex: none; }
  .due.over .dot { background: var(--danger); }
  .dtext { flex: 1; min-width: 0; display: grid; font-size: var(--t-s); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dtext span, .dtext small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dtext small { color: var(--faint); font-size: var(--t-xs); }
  .dwhen { flex: none; font-size: var(--t-xs); color: var(--muted); }
  .due.over .dwhen { color: var(--danger); }

  .col-edit { display: grid; grid-template-rows: auto auto 1fr; grid-template-columns: minmax(0, 1fr); min-height: 0; min-width: 0; }
  .docbar { display: flex; align-items: center; gap: 10px; padding: 0 8px 0 16px; min-height: 40px; border-bottom: 1px solid var(--line); font-size: var(--t-xs); min-width: 0; }
  .docbar .btn { height: 28px; font-size: var(--t-s); }
  .docbar .btn.on { color: var(--text); background: var(--raise); }
  .right { display: flex; gap: 2px; margin-left: auto; flex: none; }
  .count { min-width: 16px; height: 16px; padding: 0 4px; border-radius: 8px; background: var(--text); color: var(--bg); font-size: 10px; font-weight: 600; display: inline-grid; place-items: center; }
  .chip { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 8px; border-radius: var(--r-s); color: var(--muted); box-shadow: inset 0 0 0 1px var(--line); white-space: nowrap; flex: none; max-width: 160px; overflow: hidden; text-overflow: ellipsis; }
  .chip:hover { color: var(--text); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .chip.ghost { box-shadow: none; }
  .tags { display: flex; align-items: center; gap: 4px; min-width: 0; overflow: hidden; }
  .tag { display: inline-flex; align-items: center; gap: 2px; height: 22px; padding: 0 2px 0 7px; border-radius: 11px; background: var(--raise); color: var(--muted); white-space: nowrap; }
  .tag button { display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; color: var(--faint); }
  .tag button:hover { color: var(--text); background: var(--line); }
  .taginput { width: 120px; height: 24px; padding: 0 8px; border: 1px solid var(--muted); border-radius: var(--r-s); background: var(--sunk); font-size: var(--t-xs); user-select: text; }
  .saved { white-space: nowrap; flex: none; }
  .status { display: inline-flex; align-items: center; gap: 7px; color: var(--muted); white-space: nowrap; min-width: 0; overflow: hidden; }
  .status i { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); flex: none; box-shadow: 0 0 0 3px rgba(159, 181, 154, 0.12); }
  .status.off i { background: var(--faint); box-shadow: none; }
  .st-text { overflow: hidden; text-overflow: ellipsis; }
  .peers { display: inline-flex; flex: none; }
  .av { width: 22px; height: 22px; border-radius: 50%; display: inline-grid; place-items: center; font: 600 9.5px/1 var(--font); color: #151514; box-shadow: 0 0 0 2px var(--bg); flex: none; }
  .av.sm { width: 20px; height: 20px; font-size: 9px; box-shadow: none; }
  .peers .av + .av { margin-left: -5px; }
  .av.more { background: var(--raise); color: var(--muted); }
  .banner { display: flex; align-items: center; gap: 10px; padding: 8px 16px; border-bottom: 1px solid var(--line); background: var(--raise); font-size: var(--t-s); color: var(--muted); }
  .banner span { flex: 1; }
  .banner .btn { height: 28px; }
  .loading { display: flex; gap: 6px; justify-content: center; align-items: center; }
  .loading span { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); animation: dot 1s ease-in-out infinite; }
  .loading span:nth-child(2) { animation-delay: 0.15s; }
  .loading span:nth-child(3) { animation-delay: 0.3s; }
  @keyframes dot { 50% { opacity: 0.25; transform: translateY(-2px); } }
  .empty { margin: auto; padding: 40px; display: grid; gap: 12px; justify-items: center; color: var(--muted); }
  .empty p { margin: 0; }
  .locked { margin: auto; width: min(320px, 90%); display: grid; gap: 10px; justify-items: center; text-align: center; padding: 40px 0; }
  .locked .field { width: 100%; text-align: center; }
  .locked .btn { width: 100%; justify-content: center; }
  .lk { width: 48px; height: 48px; border-radius: 50%; display: grid; place-items: center; background: var(--raise); color: var(--muted); }
  .locked h2 { margin: 4px 0 0; font: 600 var(--t-l)/1.3 var(--font-display); }
  .locked p { margin: 0; font-size: var(--t-s); }

  .side { border-left: 1px solid var(--line); display: grid; grid-template-rows: auto 1fr; min-height: 0; min-width: 0; background: var(--sunk); }
  .sidehead { display: flex; align-items: center; justify-content: space-between; padding: 0 8px 0 14px; height: 40px; border-bottom: 1px solid var(--line); font-size: var(--t-s); color: var(--muted); }
  .clist { overflow-y: auto; padding: 10px; display: grid; gap: 8px; align-content: start; }
  .ccard { display: grid; gap: 8px; padding: 10px; border-radius: var(--r-s); background: var(--bg); border: 1px solid var(--line); transition: border-color 200ms var(--ease); }
  .ccard.flash { border-color: var(--text); }
  .ccard.done { opacity: 0.7; }
  .quote { margin: 0; padding-left: 8px; border-left: 2px solid rgba(214, 194, 122, 0.6); color: var(--muted); font-size: var(--t-xs); line-height: 1.45; text-align: left; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  button.quote:hover { color: var(--text); }
  .cmsg { display: flex; gap: 8px; align-items: flex-start; font-size: var(--t-s); }
  .cmsg b { font-weight: 600; }
  .cmsg .faint { font-size: var(--t-xs); }
  .cmsg p, .ctext { margin: 2px 0 0; line-height: 1.5; white-space: pre-wrap; word-break: break-word; user-select: text; }
  .ctext { font-size: var(--t-s); }
  .cmsg.reply { padding-left: 12px; }
  .cacts { display: flex; justify-content: flex-end; gap: 6px; }
  .cacts.left { justify-content: flex-start; gap: 4px; margin-left: -4px; }
  .cacts .btn { height: 28px; font-size: var(--t-s); }
  .ccard textarea { resize: vertical; font-size: var(--t-s); }
  .mchips { display: flex; flex-wrap: wrap; gap: 4px; }
  .mchips button { font-size: var(--t-xs); color: var(--muted); padding: 2px 7px; border-radius: 10px; box-shadow: inset 0 0 0 1px var(--line-strong); }
  .mchips button:hover { color: var(--text); }
  .dropzone { display: flex; flex-wrap: wrap; gap: 4px; padding: 8px 12px; border-bottom: 1px solid var(--line); background: var(--sunk); align-items: center; }
  .dz-h { width: 100%; font-size: var(--t-xs); color: var(--faint); }
  .dz { display: inline-flex; align-items: center; gap: 5px; padding: 4px 9px; border-radius: var(--r-s); border: 1px dashed var(--line-strong); font-size: var(--t-xs); color: var(--muted); }
  .dz.over { border-style: solid; border-color: var(--text); color: var(--text); background: var(--raise); }
  .resolvedtoggle { justify-self: start; margin-top: 6px; }

  .dlg { display: grid; gap: 12px; }
  .dlg p { margin: 0; line-height: 1.55; font-size: var(--t-s); }
  .mono { font-family: var(--mono); font-size: 12px; }
  .err { color: var(--danger); font-size: var(--t-s); margin: 0; }
  .dlg-actions { display: flex; justify-content: flex-end; gap: 6px; margin-top: 4px; }

  .vnone { margin: 0; line-height: 1.55; }
  .vgrid { display: grid; grid-template-columns: 200px minmax(0, 1fr); gap: 16px; min-height: 340px; }
  .vlist { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; align-content: start; max-height: 56vh; overflow-y: auto; }
  .vlist button { width: 100%; display: grid; text-align: left; padding: 7px 10px; border-radius: var(--r-s); font-size: var(--t-s); }
  .vlist button:hover { background: var(--raise); }
  .vlist button.on { background: var(--text); color: var(--bg); }
  .vlist small { font-size: var(--t-xs); color: var(--faint); }
  .vlist button.on small { color: var(--on-text-muted); }
  .vprev { display: grid; grid-template-rows: minmax(0, 1fr) auto; gap: 10px; min-width: 0; }
  .vdoc { max-height: 50vh; overflow-y: auto; padding: 14px 16px; border: 1px solid var(--line); border-radius: var(--r-s); background: var(--sunk); font-size: 13px; line-height: 1.6; user-select: text; }
  .vdoc :global(h1) { font: 600 20px/1.3 var(--font-display); margin: 0 0 0.5em; }
  .vdoc :global(h2) { font: 600 16px/1.3 var(--font-display); margin: 1.2em 0 0.3em; }
  .vdoc :global(h3) { font: 600 14px/1.3 var(--font-display); margin: 1em 0 0.3em; }
  .vdoc :global(p) { margin: 0.3em 0; }
  .vdoc :global(ul), .vdoc :global(ol) { padding-left: 1.3em; margin: 0.3em 0; }
  .vdoc :global(ul.tasks) { list-style: none; padding-left: 0.2em; }
  .vdoc :global(ul.tasks li) { display: flex; gap: 8px; }
  .vdoc :global(.box) { width: 12px; height: 12px; border: 1px solid var(--muted); border-radius: 3px; margin-top: 0.35em; flex: none; }
  .vdoc :global(li.done .box) { background: var(--text); }
  .vdoc :global(li.done > div) { color: var(--faint); text-decoration: line-through; }
  .vdoc :global(img) { max-width: 100%; border-radius: var(--r-s); }
  .vdoc :global(table) { border-collapse: collapse; width: 100%; }
  .vdoc :global(td), .vdoc :global(th) { border: 1px solid var(--line-strong); padding: 4px 7px; }
  .vdoc :global(.deadline) { padding: 0 6px; border-radius: 4px; background: var(--raise); font-size: 0.86em; }
  .vdoc :global(pre) { background: var(--bg); padding: 8px 10px; border-radius: var(--r-s); overflow-x: auto; font-family: var(--mono); font-size: 12px; }

  @media (max-width: 1180px) {
    .split.withside { grid-template-columns: 240px minmax(0, 1fr) 260px; }
  }
  @media (max-width: 980px) {
    .split { grid-template-columns: 230px minmax(0, 1fr); }
    .split.withside { grid-template-columns: 0 minmax(0, 1fr) 260px; }
    .split.withside .col-list { display: none; }
    .bl, .st-text, .saved { display: none; }
  }
  @media (max-width: 700px) {
    .vgrid { grid-template-columns: minmax(0, 1fr); }
    .vlist { max-height: 140px; }
  }
</style>
