<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import * as Y from "yjs";
  import { api, type Settings } from "../lib/api";
  import { t, ago } from "../lib/i18n.svelte";
  import { search } from "../lib/fuzzy";
  import { looksLikeCode } from "../lib/transforms";
  import { ClavisProvider, colorFor, fromB64, type Peer } from "../lib/collab";
  import { sharedError } from "../lib/sharedError";
  import { toast } from "../lib/toast.svelte";
  import { ask } from "../lib/dialog.svelte";
  import { nav } from "../lib/nav.svelte";
  import Icon from "../lib/Icon.svelte";
  import ShareModal from "./ShareModal.svelte";
  import SnipFolderSelect from "../lib/SnipFolderSelect.svelte";

  // Командный набор сниппетов: общий документ, где каждый сниппет — запись с названием, текстом, тегами и сокращением.
  let { id, onleft }: { id: string; onleft: () => void } = $props();

  type Entry = { key: string; title: string; body: string; tags: string; abbr: string; by: string; at: number };

  let provider = $state<ClavisProvider | null>(null);
  let role = $state<"owner" | "editor">("editor");
  let myName = $state("");
  let online = $state(true);
  let peers = $state<Peer[]>([]);
  let entries = $state<Entry[]>([]);
  let selected = $state<string | null>(null);
  let draft = $state<Entry | null>(null);
  let query = $state("");
  let title = $state("");
  let settings = $state<Settings | null>(null);
  let shareOpen = $state(false);
  let error = $state("");
  let map: Y.Map<Y.Map<unknown>> | null = null;
  let writeTimer: ReturnType<typeof setTimeout> | undefined;

  const visible = $derived(search(entries, query, (e) => `${e.title} ${e.abbr} ${e.tags} ${e.body}`));
  const abbrConflict = $derived(!!draft?.abbr && entries.some((e) => e.key !== draft!.key && e.abbr && e.abbr === draft!.abbr));

  function read(): Entry[] {
    if (!map) return [];
    const out: Entry[] = [];
    map.forEach((m, key) => {
      if (!(m instanceof Y.Map)) return;
      const g = (f: string) => String(m.get(f) ?? "");
      out.push({ key, title: g("title"), body: g("body"), tags: g("tags"), abbr: g("abbr"), by: g("by"), at: Number(m.get("at") ?? 0) });
    });
    return out.sort((a, b) => a.title.localeCompare(b.title));
  }

  onMount(async () => {
    settings = await api.settings().catch(() => null);
    const meta = (await api.sharedList().catch(() => [])).find((s) => s.id === id);
    title = meta?.title ?? "";
    try {
      const o = await api.sharedOpen(id);
      role = o.role;
      myName = o.name;
      online = o.online;
      const p = new ClavisProvider(id, fromB64(o.state), { name: o.name, color: colorFor(o.me || o.name) });
      p.awareness.on("change", () => (peers = p.peers()));
      map = p.doc.getMap("snippets") as Y.Map<Y.Map<unknown>>;
      map.observeDeep(() => {
        entries = read();
        // Чужая правка открытого сниппета: показываем её, если мы сейчас его не редактируем.
        if (draft && !document.activeElement?.closest(".col-edit")) {
          const fresh = entries.find((e) => e.key === draft!.key);
          if (fresh) draft = { ...fresh };
          else { draft = null; selected = null; }
        }
      });
      provider = p;
      entries = read();
      const tg = nav.target;
      const want = tg?.kind === "snippet" && tg.id.startsWith(`team:${id}:`) ? tg.id.slice(`team:${id}:`.length) : null;
      if (want) nav.target = null;
      const first = entries.find((e) => e.key === want) ?? entries[0];
      if (first) pick(first);
    } catch (e) {
      error = sharedError(e);
    }
  });

  onDestroy(() => {
    write();
    if (provider) {
      provider.destroy();
      api.sharedClose(id).catch(() => {});
    }
  });

  function pick(e: Entry) {
    write();
    selected = e.key;
    draft = { ...e };
  }

  function schedule() {
    clearTimeout(writeTimer);
    writeTimer = setTimeout(write, 350);
  }

  function write() {
    clearTimeout(writeTimer);
    if (!draft || !map || !provider) return;
    const m = map.get(draft.key);
    if (!(m instanceof Y.Map)) return;
    const d = draft;
    provider.doc.transact(() => {
      for (const f of ["title", "body", "tags", "abbr"] as const) if (m.get(f) !== d[f]) m.set(f, d[f]);
      m.set("by", myName);
      m.set("at", Date.now());
    });
  }

  async function create(from?: Partial<Entry>) {
    if (!map || !provider) return;
    write();
    const key = crypto.randomUUID();
    const m = new Y.Map<unknown>();
    provider.doc.transact(() => {
      map!.set(key, m);
      m.set("title", from?.title ?? t("lib.untitled"));
      m.set("body", from?.body ?? "");
      m.set("tags", from?.tags ?? "");
      m.set("abbr", from?.abbr ?? "");
      m.set("by", myName);
      m.set("at", Date.now());
    });
    entries = read();
    const e = entries.find((x) => x.key === key);
    if (e) pick(e);
    if (!from) {
      await tick();
      document.querySelector<HTMLInputElement>(".ed-title")?.select();
    }
  }

  function remove() {
    if (!draft || !map) return;
    const gone = { ...draft };
    map.delete(gone.key);
    draft = null;
    selected = null;
    entries = read();
    if (entries[0]) pick(entries[0]);
    toast(t("sn.deleted", { t: gone.title || t("lib.untitled") }), 6000, { label: t("common.undo"), run: () => create(gone) });
  }

  async function renameSet() {
    const v = title.trim();
    if (!v) return;
    await api.sharedTitle(id, v).catch(() => {});
  }

  const initials = (name: string) => name.trim().split(/\s+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? "").join("") || "?";
</script>

{#if error}
  <div class="err-wrap">
    <p>{error}</p>
    <button class="btn" onclick={onleft}>{t("common.back")}</button>
  </div>
{:else}
  <div class="split">
    <section class="col-list">
      <div class="narrowsel"><SnipFolderSelect /></div>
      <div class="sethead">
        <input class="settitle" bind:value={title} onblur={renameSet} onkeydown={(e) => { if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur(); }} disabled={role !== "owner"} placeholder={t("sn.setName")} spellcheck="false" />
        <span class="setmeta">
          <span class="status" class:off={!online}><i></i></span>
          {#each peers.slice(0, 4) as p (p.clientId)}<span class="av" style="background:{p.color}" title={p.name}>{initials(p.name)}</span>{/each}
          <button class="mini" onclick={() => (shareOpen = true)} title={role === "owner" ? t("sh.invite") : t("sh.members")} aria-label={t("sh.members")}><Icon name="users" size={14} /></button>
        </span>
      </div>
      <div class="listhead">
        <div class="searchbox"><Icon name="search" /><input bind:value={query} placeholder={t("app.search")} spellcheck="false" /></div>
        <button class="btn icon-only" onclick={() => create()} title={t("sn.add")} aria-label={t("sn.add")} disabled={!provider}><Icon name="plus" /></button>
      </div>
      <div class="items">
        {#each visible as e (e.key)}
          <button class="item" class:on={selected === e.key} onclick={() => pick(e)}>
            <span class="it-title">{e.title || t("lib.untitled")}</span>
            <span class="it-sub">{e.body.replace(/\s+/g, " ").slice(0, 90)}</span>
            <span class="it-meta">
              {#if e.abbr}<span>{settings?.expander.prefix}{e.abbr}</span>{/if}
              {#each e.tags.split(",").map((x) => x.trim()).filter(Boolean) as tag}<span>#{tag}</span>{/each}
            </span>
          </button>
        {:else}
          <p class="empty">{query ? t("picker.empty.search", { q: query }) : t("sn.teamHelp")}</p>
        {/each}
      </div>
    </section>

    <section class="col-edit">
      {#if draft}
        <div class="edhead">
          <input class="ed-title" bind:value={draft.title} oninput={schedule} onblur={write} placeholder={t("lib.title")} spellcheck="false" />
          <button class="btn ghost" aria-label={t("lib.delete")} title={t("lib.delete")} onclick={remove}><Icon name="trash" /></button>
        </div>
        <textarea class="ed-body" class:code={looksLikeCode(draft.body)} bind:value={draft.body} oninput={schedule} onblur={write} placeholder={t("lib.body")} spellcheck="false"></textarea>
        <p class="help">{t("lib.varsHelp")}</p>
        <div class="props">
          <label>
            <span>{t("lib.abbr")}</span>
            <div class="abbrfield">
              <span class="prefix">{settings?.expander.prefix}</span>
              <input class="field" bind:value={draft.abbr} oninput={schedule} onblur={write} spellcheck="false" />
            </div>
            {#if abbrConflict}<small class="warn">{t("lib.abbrTaken")}</small>{/if}
          </label>
          <label>
            <span>{t("lib.tags")}</span>
            <input class="field" bind:value={draft.tags} oninput={schedule} onblur={write} spellcheck="false" />
          </label>
        </div>
        {#if draft.by}<p class="stamp faint">{t("sn.editedBy", { n: draft.by, t: ago(draft.at) })}</p>{/if}
      {:else if provider}
        <div class="empty center">
          <p>{t("sn.teamHelp")}</p>
          <button class="btn" onclick={() => create()}><Icon name="plus" />{t("sn.add")}</button>
        </div>
      {/if}
    </section>
  </div>
{/if}

{#if shareOpen}
  <ShareModal {id} {role} kind="snippets" onclose={() => (shareOpen = false)} onleft={() => { shareOpen = false; onleft(); }} />
{/if}

<style>
  .split { display: grid; grid-template-columns: 300px minmax(0, 1fr); height: 100%; min-height: 0; }
  .narrowsel { padding: 12px 12px 0; display: none; }
  @media (max-width: 980px) { .narrowsel { display: block; } }
  .col-list { border-right: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; }
  .sethead { display: flex; align-items: center; gap: 8px; padding: 12px 12px 0; }
  .settitle { flex: 1; min-width: 0; background: none; border: 0; font: 600 var(--t-m)/1.3 var(--font-display); padding: 2px 0; user-select: text; }
  .settitle:disabled { color: var(--text); }
  .setmeta { display: flex; align-items: center; gap: 4px; flex: none; }
  .status i { display: block; width: 7px; height: 7px; border-radius: 50%; background: var(--ok); margin-right: 4px; }
  .status.off i { background: var(--faint); }
  .av { width: 20px; height: 20px; border-radius: 50%; display: inline-grid; place-items: center; font: 600 9px/1 var(--font); color: #151514; }
  .mini { color: var(--faint); padding: 5px; border-radius: var(--r-s); display: grid; }
  .mini:hover { color: var(--text); background: var(--raise); }
  .listhead { display: flex; gap: 8px; padding: 10px 12px 12px; border-bottom: 1px solid var(--line); }
  .icon-only { padding: 0 8px; }
  .searchbox { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; padding: 0 8px; border: 1px solid var(--line); border-radius: var(--r-s); color: var(--faint); background: var(--sunk); }
  .searchbox input { flex: 1; min-width: 0; background: none; border: 0; height: 28px; user-select: text; }
  .items { overflow-y: auto; padding: 6px; flex: 1; min-height: 0; }
  .item { width: 100%; min-width: 0; display: grid; grid-template-columns: minmax(0, 1fr); gap: 2px; text-align: left; padding: 9px 10px; border-radius: var(--r-s); }
  .item:hover { background: var(--raise); }
  .item.on { background: var(--text); color: var(--bg); }
  .item.on .it-sub, .item.on .it-meta { color: var(--on-text-muted); }
  .it-title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .it-sub { font-size: var(--t-s); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .it-meta { display: flex; gap: 8px; font-size: var(--t-xs); color: var(--faint); }
  .it-meta:empty { display: none; }
  .col-edit { overflow-y: auto; padding: 22px clamp(16px, 3vw, 32px) 32px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .col-edit > * { max-width: 760px; width: 100%; }
  .edhead { display: flex; align-items: center; gap: 8px; }
  .ed-title { flex: 1; background: none; border: 0; font: 600 var(--t-xl)/1.3 var(--font-display); padding: 2px 0; user-select: text; }
  .ed-body { min-height: 200px; resize: vertical; padding: 12px 14px; line-height: 1.6; background: var(--sunk); border: 1px solid var(--line); border-radius: var(--r-s); user-select: text; }
  .ed-body:focus { border-color: var(--muted); }
  .ed-body.code { font-family: var(--mono); font-size: 12.5px; }
  .help { margin: 0; font-size: var(--t-s); color: var(--faint); max-width: 70ch; }
  .props { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 14px; margin-top: 12px; }
  .props label { display: grid; gap: 5px; font-size: var(--t-s); color: var(--muted); align-content: start; }
  .props small.warn { color: var(--danger); font-size: var(--t-xs); }
  .abbrfield { display: flex; align-items: center; }
  .prefix { padding: 0 8px; height: 35px; display: grid; place-items: center; border: 1px solid var(--line); border-right: 0; border-radius: var(--r-s) 0 0 var(--r-s); color: var(--muted); background: var(--raise); }
  .abbrfield .field { border-radius: 0 var(--r-s) var(--r-s) 0; }
  .stamp { font-size: var(--t-xs); margin: 8px 0 0; }
  .empty { color: var(--muted); padding: 20px 10px; margin: 0; line-height: 1.55; font-size: var(--t-s); }
  .empty.center { margin: auto; text-align: center; display: grid; gap: 12px; justify-items: center; max-width: 46ch; }
  .empty.center p { margin: 0; }
  .err-wrap { height: 100%; display: grid; place-content: center; gap: 12px; justify-items: center; color: var(--muted); padding: 24px; text-align: center; }
  @media (max-width: 980px) { .split { grid-template-columns: 230px minmax(0, 1fr); } }
</style>
