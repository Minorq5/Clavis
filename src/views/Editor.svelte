<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { Editor } from "@tiptap/core";
  import StarterKit from "@tiptap/starter-kit";
  import { TaskList } from "@tiptap/extension-task-list";
  import { TaskItem } from "@tiptap/extension-task-item";
  import { Table, TableRow, TableCell, TableHeader } from "@tiptap/extension-table";
  import { Image } from "@tiptap/extension-image";
  import { Highlight } from "@tiptap/extension-highlight";
  import { Placeholder } from "@tiptap/extension-placeholder";
  import { Collaboration } from "@tiptap/extension-collaboration";
  import { CollaborationCaret } from "@tiptap/extension-collaboration-caret";
  import { t, i18n, type Key } from "../lib/i18n.svelte";
  import { api } from "../lib/api";
  import type { ClavisProvider } from "../lib/collab";
  import { Deadline, formatDeadline, toLocalInput } from "../lib/deadline";
  import { CommentMark } from "../lib/comment";
  import { NoteLink, TimeLog, type LinkTarget } from "../lib/nodes";
  import { search as fuzzy } from "../lib/fuzzy";
  import { openMenu } from "../lib/menu.svelte";
  import Modal from "../lib/Modal.svelte";
  import type { ClipItem } from "../lib/api";
  import { askText } from "../lib/dialog.svelte";
  import * as Y from "yjs";
  import { yXmlFragmentToProseMirrorRootNode } from "@tiptap/y-tiptap";

  let {
    doc = null,
    onchange,
    collab = null,
    readonly = false,
    onaddcomment,
    oncommentclick,
    links = [],
    onopenlink,
  }: {
    links?: LinkTarget[];
    onopenlink?: (t: { id: string; kind: "note" | "shared" }) => void;
    doc?: unknown;
    onchange: (doc: unknown, title: string, html: string) => void;
    collab?: { provider: ClavisProvider; docId: string } | null;
    readonly?: boolean;
    onaddcomment?: (id: string, quote: string) => void;
    oncommentclick?: (id: string) => void;
  } = $props();

  // В общих заметках изображения хранятся у каждого участника, а в документе лежит только ссылка на них.
  const ResolvedImage = Image.extend({
    addNodeView() {
      return ({ node }) => {
        const img = document.createElement("img");
        let current = "";
        const set = (src: string) => {
          if (src === current) return;
          current = src;
          if (src.startsWith("clavis-img:") && collab) {
            img.classList.add("loading");
            api
              .sharedImageGet(collab.docId, src.slice("clavis-img:".length))
              .then((url) => { if (current === src) img.src = url; })
              .catch(() => img.classList.add("missing"))
              .finally(() => img.classList.remove("loading"));
          } else {
            img.src = src;
          }
        };
        set(node.attrs.src);
        if (node.attrs.alt) img.alt = node.attrs.alt;
        return {
          dom: img,
          update: (n) => {
            if (n.type.name !== "image") return false;
            set(n.attrs.src);
            return true;
          },
        };
      };
    },
  });

  async function placeImage(dataUrl: string) {
    if (!editor) return;
    if (collab) {
      try {
        const src = await api.sharedImagePut(collab.docId, dataUrl);
        editor.chain().focus().setImage({ src }).run();
      } catch {
        /* слишком большое или повреждённое изображение не вставляем */
      }
    } else {
      editor.chain().focus().setImage({ src: dataUrl }).run();
    }
  }

  let host: HTMLDivElement;
  let editor = $state<Editor | null>(null);
  let tickN = $state(0); // перерисовка состояния кнопок
  let slash = $state<{ q: string; from: number; x: number; y: number; i: number } | null>(null);
  let due = $state<{ pos: number; x: number; y: number; value: string } | null>(null);
  let linkPop = $state<{ q: string; from: number; x: number; y: number; i: number } | null>(null);
  let imgPick = $state<ClipItem[] | null>(null);
  const linkItems = $derived(linkPop ? fuzzy(links, linkPop.q, (l) => l.title).slice(0, 8) : []);

  function insertLink(l: LinkTarget) {
    if (!editor || !linkPop) return;
    const to = editor.state.selection.from;
    editor.chain().focus().deleteRange({ from: linkPop.from, to }).insertContent([{ type: "noteLink", attrs: { id: l.id, kind: l.kind, title: l.title } }, { type: "text", text: " " }]).run();
    linkPop = null;
  }

  function imageMenu(e: MouseEvent) {
    openMenu(e.currentTarget as HTMLElement, [
      { label: t("ed.fromFile"), icon: "files", run: () => editor && insertImage(editor) },
      { label: t("ed.fromHistory"), icon: "history", run: openHistoryImages },
    ]);
  }

  async function openHistoryImages() {
    imgPick = (await api.history().catch(() => [])).filter((h) => h.kind === "image");
  }

  async function pickHistoryImage(h: ClipItem) {
    imgPick = null;
    const url = await api.historyImage(h.id).catch(() => "");
    if (url) await placeImage(url);
  }

  function openDeadline(pos: number, rect: DOMRect) {
    if (!editor || readonly) return;
    const node = editor.state.doc.nodeAt(pos);
    if (!node || node.type.name !== "deadline") return;
    const box = host.getBoundingClientRect();
    due = { pos, x: Math.min(rect.left - box.left, box.width - 260), y: rect.bottom - box.top + host.scrollTop + 6, value: toLocalInput(node.attrs.at) };
  }

  function applyDeadline() {
    if (!editor || !due) return;
    const at = new Date(due.value).getTime();
    const node = editor.state.doc.nodeAt(due.pos);
    if (Number.isFinite(at) && node?.type.name === "deadline") editor.view.dispatch(editor.state.tr.setNodeMarkup(due.pos, undefined, { at }));
    due = null;
    editor.commands.focus();
  }

  function removeDeadline() {
    if (!editor || !due) return;
    const node = editor.state.doc.nodeAt(due.pos);
    if (node?.type.name === "deadline") editor.view.dispatch(editor.state.tr.delete(due.pos, due.pos + node.nodeSize));
    due = null;
    editor.commands.focus();
  }

  function insertDeadline(e: Editor) {
    const d = new Date();
    d.setDate(d.getDate() + 1);
    d.setHours(9, 0, 0, 0);
    const pos = e.state.selection.from;
    e.chain().focus().insertContent([{ type: "deadline", attrs: { at: d.getTime() } }, { type: "text", text: " " }]).run();
    tick().then(() => {
      const dom = e.view.nodeDOM(pos) as HTMLElement | null;
      if (dom) openDeadline(pos, dom.getBoundingClientRect());
    });
  }

  function requestComment() {
    if (!editor || !onaddcomment) return;
    const { from, to, empty } = editor.state.selection;
    if (empty) return;
    const id = crypto.randomUUID();
    const quote = editor.state.doc.textBetween(from, to, " ").slice(0, 300);
    editor.chain().setMark("comment", { id }).run();
    onaddcomment(id, quote);
  }

  type Block = { id: string; key: Key; hint: string; run: (e: Editor) => void };
  const blocks: Block[] = [
    { id: "p", key: "ed.text", hint: "", run: (e) => e.chain().focus().setParagraph().run() },
    { id: "h1", key: "ed.h1", hint: "#", run: (e) => e.chain().focus().setHeading({ level: 1 }).run() },
    { id: "h2", key: "ed.h2", hint: "##", run: (e) => e.chain().focus().setHeading({ level: 2 }).run() },
    { id: "h3", key: "ed.h3", hint: "###", run: (e) => e.chain().focus().setHeading({ level: 3 }).run() },
    { id: "ul", key: "ed.bullet", hint: "-", run: (e) => e.chain().focus().toggleBulletList().run() },
    { id: "ol", key: "ed.ordered", hint: "1.", run: (e) => e.chain().focus().toggleOrderedList().run() },
    { id: "task", key: "ed.task", hint: "[ ]", run: (e) => e.chain().focus().toggleTaskList().run() },
    { id: "quote", key: "ed.quote", hint: ">", run: (e) => e.chain().focus().toggleBlockquote().run() },
    { id: "code", key: "ed.codeblock", hint: "```", run: (e) => e.chain().focus().toggleCodeBlock().run() },
    { id: "table", key: "ed.table", hint: "", run: (e) => e.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run() },
    { id: "image", key: "ed.image", hint: "", run: (e) => insertImage(e) },
    { id: "hr", key: "ed.divider", hint: "---", run: (e) => e.chain().focus().setHorizontalRule().run() },
    { id: "due", key: "ed.deadline", hint: "@30.11", run: (e) => insertDeadline(e) },
    { id: "link", key: "ed.noteLink", hint: "[[", run: (e) => e.chain().focus().insertContent("[[").run() },
    { id: "timer", key: "ed.timer", hint: "", run: (e) => e.chain().focus().insertContent({ type: "timeLog", attrs: { entries: [] } }).run() },
    { id: "img2", key: "ed.fromHistory", hint: "", run: () => openHistoryImages() },
  ];

  const slashItems = $derived(
    slash ? blocks.filter((b) => !slash!.q || t(b.key).toLocaleLowerCase().includes(slash!.q.toLocaleLowerCase())) : [],
  );

  async function insertImage(_e: Editor) {
    const src = await api.noteImage();
    if (src) await placeImage(src);
  }

  function readFileAsDataUrl(f: File): Promise<string> {
    return new Promise((res) => {
      const r = new FileReader();
      r.onload = () => res(String(r.result));
      r.readAsDataURL(f);
    });
  }

  function titleOf(e: Editor) {
    const first = e.state.doc.firstChild?.textContent?.trim();
    return first || "";
  }

  function checkLinkPop(e: Editor) {
    const { $from: from$, empty } = e.state.selection;
    if (!empty || from$.parent.type.name === "codeBlock") return (linkPop = null);
    const before = from$.parent.textBetween(0, from$.parentOffset, undefined, "￼");
    const m = before.match(/\[\[([^\[\]\n]{0,40})$/);
    if (!m) return (linkPop = null);
    const from = from$.pos - m[1].length - 2;
    const c = e.view.coordsAtPos(from);
    const box = host.getBoundingClientRect();
    const prev = linkPop;
    linkPop = { q: m[1], from, x: c.left - box.left, y: c.bottom - box.top + host.scrollTop + 6, i: prev && prev.q === m[1] ? prev.i : 0 };
  }

  function checkSlash(e: Editor) {
    checkLinkPop(e);
    const { $from: from$, empty } = e.state.selection;
    if (!empty) return (slash = null);
    const before = from$.parent.textBetween(0, from$.parentOffset, undefined, "￼");
    const m = before.match(/(?:^|\s)\/([\p{L}\d]*)$/u);
    if (!m || from$.parent.type.name === "codeBlock") return (slash = null);
    const from = from$.pos - m[1].length - 1;
    const c = e.view.coordsAtPos(from);
    const box = host.getBoundingClientRect();
    const prev = slash;
    slash = { q: m[1], from, x: c.left - box.left, y: c.bottom - box.top + host.scrollTop + 6, i: prev && prev.q === m[1] ? prev.i : 0 };
  }

  function runSlash(b: Block) {
    if (!editor || !slash) return;
    const to = editor.state.selection.from;
    editor.chain().focus().deleteRange({ from: slash.from, to }).run();
    slash = null;
    b.run(editor);
  }

  onMount(() => {
    editor = new Editor({
      element: host,
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
          link: { openOnClick: false, autolink: true },
          // В общей заметке отменой правок занимается Yjs: отменяются только свои изменения.
          // Пустой абзац в конце каждый участник добавлял бы сам, и они размножались бы.
          ...(collab ? { undoRedo: false, trailingNode: false } : {}),
        }),
        ...(collab
          ? [
              Collaboration.configure({ document: collab.provider.doc, field: "default" }),
              CollaborationCaret.configure({ provider: collab.provider, user: collab.provider.user }),
            ]
          : []),
        TaskList,
        TaskItem.configure({ nested: true }),
        Table.configure({ resizable: true }),
        TableRow,
        TableHeader,
        TableCell,
        ResolvedImage.configure({ allowBase64: true }),
        Highlight,
        Placeholder.configure({ placeholder: t("ed.placeholder") }),
        Deadline.configure({ onClick: (pos, rect) => openDeadline(pos, rect) }),
        NoteLink.configure({ resolve: (id) => links.find((l) => l.id === id) ?? null, onOpen: (tg) => onopenlink?.(tg) }),
        TimeLog,
        CommentMark,
      ],
      editable: !readonly,
      ...(collab ? {} : { content: doc as object }),
      editorProps: {
        attributes: { class: "prose", spellcheck: "true" },
        handleClick: (view, pos) => {
          if (!oncommentclick) return false;
          const m = view.state.doc.resolve(pos).marks().find((x) => x.type.name === "comment");
          if (m) oncommentclick(m.attrs.id);
          return false;
        },
        handleKeyDown: (_view, ev) => {
          if (linkPop) {
            if (ev.key === "Escape") { linkPop = null; return true; }
            if (linkItems.length) {
              if (ev.key === "ArrowDown") { linkPop.i = (linkPop.i + 1) % linkItems.length; return true; }
              if (ev.key === "ArrowUp") { linkPop.i = (linkPop.i - 1 + linkItems.length) % linkItems.length; return true; }
              if (ev.key === "Enter" || ev.key === "Tab") { insertLink(linkItems[linkPop.i]); return true; }
            }
          }
          if (!slash || !slashItems.length) return false;
          if (ev.key === "ArrowDown") { slash.i = (slash.i + 1) % slashItems.length; return true; }
          if (ev.key === "ArrowUp") { slash.i = (slash.i - 1 + slashItems.length) % slashItems.length; return true; }
          if (ev.key === "Enter" || ev.key === "Tab") { runSlash(slashItems[slash.i]); return true; }
          if (ev.key === "Escape") { slash = null; return true; }
          return false;
        },
        handlePaste: (_view, ev) => {
          const files = [...(ev.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
          if (!files.length) return false;
          files.forEach(async (f) => placeImage(await readFileAsDataUrl(f)));
          return true;
        },
        handleDrop: (_view, ev) => {
          const files = [...((ev as DragEvent).dataTransfer?.files ?? [])].filter((f) => f.type.startsWith("image/"));
          if (!files.length) return false;
          files.forEach(async (f) => placeImage(await readFileAsDataUrl(f)));
          return true;
        },
      },
      onUpdate: ({ editor: e }) => {
        checkSlash(e);
        onchange(e.getJSON(), titleOf(e), e.getHTML());
      },
      onSelectionUpdate: ({ editor: e }) => { checkSlash(e); tickN++; },
      onTransaction: () => tickN++,
    });
  });

  onDestroy(() => editor?.destroy());

  export function focusStart() {
    editor?.commands.focus("start");
  }

  export function getJSON() {
    return editor?.getJSON();
  }

  /** Заменяет содержимое (используется, когда личную заметку делают общей). */
  export function setContent(json: unknown) {
    editor?.commands.setContent(json as object);
  }

  export function getHTML() {
    return editor?.getHTML() ?? "";
  }

  /** Убирает пометку комментария из текста. */
  export function removeComment(id: string) {
    if (!editor) return;
    const type = editor.schema.marks.comment;
    const tr = editor.state.tr;
    editor.state.doc.descendants((n, pos) => {
      const m = n.marks.find((x) => x.type === type && x.attrs.id === id);
      if (m) tr.removeMark(pos, pos + n.nodeSize, m);
    });
    if (tr.docChanged) editor.view.dispatch(tr);
  }

  /** Выделяет текст комментария и прокручивает к нему. Возвращает false, если текст удалён. */
  export function focusComment(id: string): boolean {
    if (!editor) return false;
    let from = -1, to = -1;
    editor.state.doc.descendants((n, pos) => {
      if (n.marks.some((x) => x.type.name === "comment" && x.attrs.id === id)) {
        if (from < 0) from = pos;
        to = pos + n.nodeSize;
      }
    });
    if (from < 0) return false;
    editor.chain().focus().setTextSelection({ from, to }).scrollIntoView().run();
    return true;
  }

  /** Состояние Yjs из истории версий в документ редактора. */
  export function yjsToJSON(state: Uint8Array): unknown {
    if (!editor) return null;
    const d = new Y.Doc();
    Y.applyUpdate(d, state);
    const json = yXmlFragmentToProseMirrorRootNode(d.getXmlFragment("default"), editor.schema).toJSON();
    d.destroy();
    return json;
  }

  function active(name: string, attrs?: Record<string, unknown>) {
    tickN;
    return editor?.isActive(name, attrs) ?? false;
  }

  const blockLabel = $derived.by(() => {
    tickN;
    if (!editor) return t("ed.text");
    for (const l of [1, 2, 3] as const) if (editor.isActive("heading", { level: l })) return t(`ed.h${l}` as Key);
    if (editor.isActive("codeBlock")) return t("ed.codeblock");
    if (editor.isActive("blockquote")) return t("ed.quote");
    return t("ed.text");
  });

  let blockMenu = $state(false);
  const selEmpty = $derived.by(() => {
    tickN;
    return !editor || editor.state.selection.empty;
  });

  async function setLink() {
    if (!editor) return;
    const prev = editor.getAttributes("link").href ?? "";
    const url = (await askText({ title: t("ed.link"), text: t("ed.linkPrompt"), ok: t("common.save"), value: prev, placeholder: "https://" }))?.trim() ?? null;
    if (url === null) return;
    if (!url) editor.chain().focus().unsetLink().run();
    else editor.chain().focus().extendMarkRange("link").setLink({ href: url }).run();
  }

  const c = () => editor!.chain().focus();
</script>

<div class="editor" class:ro={readonly}>
  <div class="toolbar" role="toolbar">
    <div class="blocksel">
      <button class="tb wide" onclick={() => (blockMenu = !blockMenu)} aria-haspopup="menu">{blockLabel}<svg viewBox="0 0 10 10"><path d="M2.5 4l2.5 2.5L7.5 4" /></svg></button>
      {#if blockMenu}
        <div class="menu" role="menu">
          {#each blocks.slice(0, 4).concat(blocks.slice(7, 9)) as b}
            <button role="menuitem" onclick={() => { blockMenu = false; editor && b.run(editor); }}><span>{t(b.key)}</span><small>{b.hint}</small></button>
          {/each}
        </div>
      {/if}
    </div>
    <span class="sep"></span>
    <button class="tb" class:on={active("bold")} title="{t('ed.bold')}  Ctrl+B" onclick={() => c().toggleBold().run()}><b>{i18n.lang === "ru" ? "Ж" : "B"}</b></button>
    <button class="tb" class:on={active("italic")} title="{t('ed.italic')}  Ctrl+I" onclick={() => c().toggleItalic().run()}><i>{i18n.lang === "ru" ? "К" : "I"}</i></button>
    <button class="tb" class:on={active("underline")} title="{t('ed.underline')}  Ctrl+U" onclick={() => c().toggleUnderline().run()}><u>{i18n.lang === "ru" ? "Ч" : "U"}</u></button>
    <button class="tb" class:on={active("strike")} title="{t('ed.strike')}  Ctrl+Shift+S" onclick={() => c().toggleStrike().run()}><s>{i18n.lang === "ru" ? "З" : "S"}</s></button>
    <button class="tb" class:on={active("highlight")} title="{t('ed.highlight')}  Ctrl+Shift+H" onclick={() => c().toggleHighlight().run()}><span class="hl">{i18n.lang === "ru" ? "М" : "H"}</span></button>
    <button class="tb" class:on={active("code")} title="{t('ed.code')}  Ctrl+E" onclick={() => c().toggleCode().run()}><svg viewBox="0 0 16 16"><path d="M5.5 4 2.5 8l3 4M10.5 4l3 4-3 4" /></svg></button>
    <button class="tb" class:on={active("link")} title={t("ed.link")} onclick={setLink}><svg viewBox="0 0 16 16"><path d="M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2-2a2.6 2.6 0 0 0-3.7-3.7l-.7.7M9.2 6.8a2.6 2.6 0 0 0-3.7 0l-2 2a2.6 2.6 0 0 0 3.7 3.7l.7-.7" /></svg></button>
    <span class="sep"></span>
    <button class="tb" class:on={active("bulletList")} title={t("ed.bullet")} onclick={() => c().toggleBulletList().run()}><svg viewBox="0 0 16 16"><path d="M6 4.5h7.5M6 8h7.5M6 11.5h7.5M2.8 4.5h.4M2.8 8h.4M2.8 11.5h.4" /></svg></button>
    <button class="tb" class:on={active("orderedList")} title={t("ed.ordered")} onclick={() => c().toggleOrderedList().run()}><svg viewBox="0 0 16 16"><path d="M6.5 4.5h7M6.5 8h7M6.5 11.5h7M2.5 3.5l1-.6v3M2.4 9.5h1.8L2.4 11.8h1.9" /></svg></button>
    <button class="tb" class:on={active("taskList")} title={t("ed.task")} onclick={() => c().toggleTaskList().run()}><svg viewBox="0 0 16 16"><path d="M2.5 3h3v3h-3zM2.5 10h3v3h-3zM3.2 4.5l.6.6 1.2-1.3M8 4.5h5.5M8 11.5h5.5" /></svg></button>
    <button class="tb" title={t("ed.table")} onclick={() => c().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()}><svg viewBox="0 0 16 16"><path d="M2.5 3h11v10h-11zM2.5 6.5h11M2.5 9.8h11M7 3v10" /></svg></button>
    <button class="tb" title={t("ed.image")} onclick={imageMenu} aria-haspopup="menu"><svg viewBox="0 0 16 16"><path d="M2.5 3.5h11v9h-11zM2.5 10.5l3-3 3 3 2-2 2.5 2.5" /></svg></button>
    <button class="tb" title="{t('ed.deadline')}  @30.11" onclick={() => editor && insertDeadline(editor)}><svg viewBox="0 0 16 16"><path d="M2.5 4h11v9.5h-11zM2.5 6.8h11M5.5 2.5V5M10.5 2.5V5" /></svg></button>
    {#if onaddcomment}
      <button class="tb" title={t("ed.comment")} disabled={selEmpty} onclick={requestComment}><svg viewBox="0 0 16 16"><path d="M2.5 3.5h11v7.5H7l-3 2.5V11H2.5z" /></svg></button>
    {/if}
    {#if active("table")}
      <span class="sep"></span>
      <button class="tb wide small" onclick={() => c().addRowAfter().run()}>{t("ed.rowAdd")}</button>
      <button class="tb wide small" onclick={() => c().addColumnAfter().run()}>{t("ed.colAdd")}</button>
      <button class="tb wide small" onclick={() => c().deleteRow().run()}>{t("ed.rowDel")}</button>
      <button class="tb wide small" onclick={() => c().deleteColumn().run()}>{t("ed.colDel")}</button>
    {/if}
  </div>

  <div class="page" bind:this={host}>
    {#if linkPop}
      <div class="slash" style="left:{linkPop.x}px; top:{linkPop.y}px" role="listbox">
        {#each linkItems as l, i (l.id)}
          <button role="option" aria-selected={i === linkPop.i} onmousedown={(e) => { e.preventDefault(); insertLink(l); }}>
            <span>{l.title}</span><small>{l.kind === "shared" ? t("sh.shared") : ""}</small>
          </button>
        {:else}
          <p class="nolinks">{t("ed.noNotes")}</p>
        {/each}
      </div>
    {/if}
    {#if due}
      <div class="duepop" style="left:{Math.max(0, due.x)}px; top:{due.y}px" role="dialog">
        <span class="dlabel">{t("ed.setDeadline")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="field" type="datetime-local" bind:value={due.value} autofocus onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); applyDeadline(); } if (e.key === "Escape") { e.preventDefault(); due = null; editor?.commands.focus(); } }} />
        <div class="dacts">
          <button class="btn ghost danger" onclick={removeDeadline}>{t("lib.delete")}</button>
          <button class="btn primary" onclick={applyDeadline}>{t("common.save")}</button>
        </div>
      </div>
    {/if}
    {#if slash && slashItems.length}
      <div class="slash" style="left:{slash.x}px; top:{slash.y}px" role="listbox">
        {#each slashItems as b, i}
          <button role="option" aria-selected={i === slash.i} onmousedown={(e) => { e.preventDefault(); runSlash(b); }}>
            <span>{t(b.key)}</span><small>{b.hint}</small>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if imgPick}
  <Modal title={t("ed.pickImage")} onclose={() => (imgPick = null)} width={620}>
    {#if imgPick.length}
      <div class="imggrid">
        {#each imgPick as h (h.id)}
          <button onclick={() => pickHistoryImage(h)}><img src={h.image} alt="" /><small>{h.imageSize?.join("×")}</small></button>
        {/each}
      </div>
    {:else}
      <p class="nolinks">{t("ed.noImages")}</p>
    {/if}
  </Modal>
{/if}

<svelte:window onclick={(e) => { const el = e.target as HTMLElement; if (blockMenu && !el.closest(".blocksel")) blockMenu = false; if (due && !el.closest(".duepop") && !el.closest(".deadline")) due = null; }} />

<style>
  .editor { display: grid; grid-template-rows: auto 1fr; height: 100%; min-height: 0; }
  .toolbar { display: flex; align-items: center; gap: 1px; padding: 6px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  .tb { min-width: 28px; height: 28px; padding: 0 6px; display: inline-grid; place-items: center; border-radius: var(--r-s); color: var(--muted); font-size: var(--t-m); }
  .tb:hover { color: var(--text); background: var(--raise); }
  .tb.on { color: var(--bg); background: var(--text); }
  .tb.wide { display: inline-flex; gap: 6px; padding: 0 8px; color: var(--text); }
  .tb.small { font-size: var(--t-xs); color: var(--muted); }
  .tb svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; }
  .tb.wide svg { width: 10px; height: 10px; }
  .hl { box-shadow: inset 0 -5px 0 rgba(232, 229, 222, 0.28); padding: 0 1px; }
  .sep { width: 1px; height: 16px; background: var(--line); margin: 0 6px; }

  .blocksel { position: relative; }
  .menu, .slash {
    position: absolute; z-index: 20; min-width: 210px; padding: 4px;
    background: var(--raise); border: 1px solid var(--line-strong); border-radius: var(--r-m);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }
  .menu { top: 32px; left: 0; }
  .menu button, .slash button { width: 100%; display: flex; justify-content: space-between; gap: 16px; padding: 6px 8px; border-radius: var(--r-s); text-align: left; }
  .menu button:hover, .slash button[aria-selected="true"] { background: var(--text); color: var(--bg); }
  .menu small, .slash small { color: var(--faint); font-family: var(--mono); font-size: 11px; }
  .slash button[aria-selected="true"] small, .menu button:hover small { color: var(--on-text-muted); }

  .tb:disabled { opacity: 0.35; cursor: default; background: none; }
  .ro .toolbar { display: none; }
  .ro { grid-template-rows: 1fr; }
  .duepop { position: absolute; z-index: 25; width: 250px; padding: 10px; display: grid; gap: 8px; background: var(--raise); border: 1px solid var(--line-strong); border-radius: var(--r-m); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45); }
  .duepop input { color-scheme: dark; }
  .dlabel { font-size: var(--t-xs); color: var(--faint); }
  .dacts { display: flex; justify-content: space-between; }
  .dacts .btn { height: 28px; font-size: var(--t-s); }
  .page :global(.deadline) { display: inline-flex; align-items: center; gap: 5px; padding: 0 7px; margin: 0 1px; border-radius: 4px; font-size: 0.86em; line-height: 1.6; color: var(--text); background: var(--raise); box-shadow: inset 0 0 0 1px var(--line-strong); cursor: pointer; white-space: nowrap; user-select: none; vertical-align: baseline; }
  .page :global(.deadline::before) { content: ""; width: 6px; height: 6px; border-radius: 50%; background: var(--ok); }
  .page :global(.deadline.overdue::before) { background: var(--danger); }
  .page :global(.deadline.ProseMirror-selectednode) { box-shadow: inset 0 0 0 1px var(--text); }
  .page :global(li[data-checked="true"] .deadline::before) { background: var(--faint); }
  .page :global(.cmt) { background: rgba(214, 194, 122, 0.16); border-bottom: 1px solid rgba(214, 194, 122, 0.6); cursor: pointer; }
  .nolinks { margin: 0; padding: 8px; color: var(--faint); font-size: var(--t-s); }
  .imggrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; }
  .imggrid button { display: grid; gap: 4px; padding: 6px; border-radius: var(--r-s); border: 1px solid var(--line); text-align: left; }
  .imggrid button:hover { border-color: var(--text); }
  .imggrid img { width: 100%; aspect-ratio: 16 / 10; object-fit: cover; border-radius: 3px; background: var(--sunk); }
  .imggrid small { font-size: var(--t-xs); color: var(--faint); }
  .page :global(.notelink) { padding: 0 2px; color: var(--text); border-bottom: 1px dashed var(--muted); cursor: pointer; }
  .page :global(.notelink:hover) { border-bottom-style: solid; }
  .page :global(.notelink.missing) { color: var(--faint); text-decoration: line-through; cursor: default; }
  .page :global(.timelog) { margin: 0.8em 0; padding: 10px 12px; border: 1px solid var(--line-strong); border-radius: var(--r-m); background: var(--sunk); font-size: 13px; user-select: none; }
  .page :global(.timelog.ProseMirror-selectednode) { border-color: var(--text); }
  .page :global(.tl-head) { display: flex; align-items: center; gap: 12px; }
  .page :global(.tl-name) { font-weight: 600; }
  .page :global(.tl-total) { color: var(--muted); margin-right: auto; font-variant-numeric: tabular-nums; }
  .page :global(.tl-btn) { height: 26px; padding: 0 12px; border-radius: var(--r-s); background: var(--text); color: var(--bg); font-weight: 600; font-size: 12px; }
  .page :global(.tl-btn.on) { background: var(--danger); color: #fff; }
  .page :global(.tl-list) { margin-top: 8px; display: grid; gap: 2px; }
  .page :global(.tl-row) { display: flex; align-items: center; gap: 12px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .page :global(.tl-row.live) { color: var(--text); }
  .page :global(.tl-dur) { margin-left: auto; }
  .page :global(.tl-del) { width: 20px; height: 20px; border-radius: 50%; color: var(--faint); opacity: 0; }
  .page :global(.tl-row:hover .tl-del) { opacity: 1; }
  .page :global(.tl-del:hover) { color: var(--danger); background: var(--raise); }
  .page :global(.tl-empty) { color: var(--faint); }
  .page { position: relative; overflow-y: auto; padding: 28px 40px 80px; user-select: text; }

  .page :global(.prose) { max-width: 72ch; margin: 0 auto; outline: none; line-height: 1.65; font-size: 14.5px; min-height: 60vh; }
  .page :global(.prose > * + *) { margin-top: 0.65em; }
  .page :global(.prose h1) { font: 600 26px/1.25 var(--font-display); margin: 0 0 0.6em; letter-spacing: -0.01em; }
  .page :global(.prose h2) { font: 600 19px/1.3 var(--font-display); margin-top: 1.5em; }
  .page :global(.prose h3) { font: 600 15.5px/1.35 var(--font-display); margin-top: 1.3em; }
  .page :global(.prose p) { margin: 0; }
  .page :global(.prose ul), .page :global(.prose ol) { padding-left: 1.4em; margin-bottom: 0; }
  .page :global(.prose li + li) { margin-top: 0.2em; }
  .page :global(.prose li p) { margin: 0; }
  .page :global(.prose ul[data-type="taskList"]) { list-style: none; padding-left: 0.2em; }
  .page :global(.prose ul[data-type="taskList"] li) { display: flex; gap: 10px; align-items: flex-start; }
  .page :global(.prose ul[data-type="taskList"] li > label) { margin-top: 0.22em; }
  .page :global(.prose ul[data-type="taskList"] li > div) { flex: 1; }
  .page :global(.prose ul[data-type="taskList"] li[data-checked="true"] > div) { color: var(--faint); text-decoration: line-through; text-decoration-color: var(--faint); }
  .page :global(.prose input[type="checkbox"]) {
    appearance: none; width: 15px; height: 15px; margin: 0; border: 1px solid var(--muted); border-radius: 3px; cursor: pointer; display: grid; place-items: center;
  }
  .page :global(.prose input[type="checkbox"]:checked) { background: var(--text); border-color: var(--text); }
  .page :global(.prose input[type="checkbox"]:checked::after) { content: ""; width: 7px; height: 4px; border: solid var(--bg); border-width: 0 0 1.6px 1.6px; transform: rotate(-45deg) translate(1px, -1px); }
  .page :global(.prose blockquote) { margin-left: 0; padding-left: 14px; border-left: 2px solid var(--line-strong); color: var(--muted); }
  .page :global(.prose code) { font-family: var(--mono); font-size: 0.86em; background: var(--raise); border: 1px solid var(--line); border-radius: 3px; padding: 0 4px; }
  .page :global(.prose pre) { background: var(--sunk); border: 1px solid var(--line); border-radius: var(--r-s); padding: 12px 14px; overflow-x: auto; }
  .page :global(.prose pre code) { background: none; border: 0; padding: 0; font-size: 12.5px; line-height: 1.6; }
  .page :global(.prose mark) { background: rgba(232, 229, 222, 0.2); color: var(--text); border-radius: 2px; padding: 0 2px; }
  .page :global(.prose a) { color: var(--text); text-decoration: underline; text-decoration-color: var(--faint); text-underline-offset: 3px; }
  .page :global(.prose hr) { border: 0; border-top: 1px solid var(--line); margin: 1.6em 0; }
  .page :global(.prose img) { max-width: 100%; border-radius: var(--r-s); display: block; }
  .page :global(.prose img.ProseMirror-selectednode) { outline: 1px solid var(--text); }
  .page :global(.prose table) { border-collapse: collapse; width: 100%; table-layout: fixed; overflow: hidden; }
  .page :global(.prose td), .page :global(.prose th) { border: 1px solid var(--line-strong); padding: 6px 9px; vertical-align: top; position: relative; }
  .page :global(.prose th) { background: var(--raise); font-weight: 600; text-align: left; }
  .page :global(.prose .selectedCell::after) { content: ""; position: absolute; inset: 0; background: rgba(232, 229, 222, 0.08); pointer-events: none; }
  .page :global(.prose .column-resize-handle) { position: absolute; right: -1px; top: 0; bottom: 0; width: 3px; background: var(--muted); pointer-events: none; }
  .page :global(.collaboration-carets__caret) { position: relative; margin: 0 -1px; border-left: 1.5px solid; border-right: 0; word-break: normal; pointer-events: none; }
  .page :global(.collaboration-carets__label) {
    position: absolute; left: -1.5px; top: -1.45em; padding: 1px 6px; border-radius: 3px 3px 3px 0;
    font: 600 10.5px/1.4 var(--font); color: #151514; white-space: nowrap; user-select: none; pointer-events: none;
    animation: caretlabel 2.4s var(--ease) forwards;
  }
  @keyframes caretlabel { 0%, 70% { opacity: 1; } 100% { opacity: 0.55; } }
  .page :global(.collaboration-carets__selection) { border-radius: 2px; }
  .page :global(.prose img.loading) { min-height: 120px; background: var(--raise); animation: pulse 1.2s ease-in-out infinite; }
  .page :global(.prose img.missing) { min-height: 60px; background: var(--raise); outline: 1px dashed var(--line-strong); }
  @keyframes pulse { 50% { opacity: 0.5; } }
  .page :global(.prose p.is-editor-empty:first-child::before) { content: attr(data-placeholder); color: var(--faint); float: left; height: 0; pointer-events: none; }
</style>
