<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Folder, type Snippet, type Settings } from "../lib/api";
  import { t, ago } from "../lib/i18n.svelte";
  import { search } from "../lib/fuzzy";
  import { looksLikeCode } from "../lib/transforms";
  import Icon from "../lib/Icon.svelte";
  import { nav } from "../lib/nav.svelte";
  import SnipFolderSelect from "../lib/SnipFolderSelect.svelte";
  import { toast } from "../lib/toast.svelte";

  let { folder = "all" }: { folder?: string } = $props();

  let snippets = $state<Snippet[]>([]);
  let folders = $state<Folder[]>([]);
  let settings = $state<Settings | null>(null);
  let query = $state("");
  let selected = $state<string | null>(null);
  let draft = $state<Snippet | null>(null);
  let tagsText = $state("");
  let savedAt = $state(0);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let uses = $state<Record<string, { count: number; last: number }>>({});
  let sort = $state<"recent" | "used" | "unused">("recent");

  onMount(async () => {
    [snippets, folders, settings] = await Promise.all([api.snippets(), api.folders(), api.settings()]);
    uses = await api.snippetUses().catch(() => ({}));
    const tg = nav.target;
    const want = tg?.kind === "snippet" ? snippets.find((s) => s.id === tg.id) : undefined;
    if (want) nav.target = null;
    const first = want ?? visible[0];
    if (first) pick(first);
  });

  const inFolder = $derived(
    snippets.filter((s) => (folder === "all" ? true : folder === "none" ? !s.folder : s.folder === folder)),
  );
  const found = $derived(search(inFolder, query, (s) => `${s.title} ${s.abbr} ${s.tags.join(" ")} ${s.body}`));
  // Сортировка помогает чистить библиотеку: частые наверху, неиспользуемые отдельно.
  const visible = $derived(
    sort === "used" ? [...found].sort((a, b) => (uses[b.id]?.count ?? 0) - (uses[a.id]?.count ?? 0))
    : sort === "unused" ? found.filter((s) => !uses[s.id])
    : found,
  );

  function pick(s: Snippet) {
    flush();
    selected = s.id;
    draft = structuredClone($state.snapshot(s));
    tagsText = s.tags.join(", ");
  }

  async function create() {
    flush();
    const s = await api.snippetSave({ id: "", title: t("lib.untitled"), body: "", folder: folder !== "all" && folder !== "none" ? folder : null, tags: [], abbr: "", updated: 0 });
    snippets = await api.snippets();
    pick(s);
    queueMicrotask(() => document.querySelector<HTMLInputElement>(".ed-title")?.select());
  }

  function schedule() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 500);
  }

  async function flush() {
    clearTimeout(saveTimer);
    if (!draft) return;
    const s = { ...$state.snapshot(draft), tags: tagsText.split(",").map((x) => x.trim()).filter(Boolean) };
    const before = snippets.find((x) => x.id === s.id);
    if (before && JSON.stringify({ ...before, updated: 0 }) === JSON.stringify({ ...s, updated: 0 })) return;
    await api.snippetSave(s);
    snippets = await api.snippets();
    savedAt = Date.now();
  }

  async function remove() {
    if (!draft) return;
    clearTimeout(saveTimer);
    const gone = { ...$state.snapshot(draft), tags: tagsText.split(",").map((x) => x.trim()).filter(Boolean) };
    const idx = visible.findIndex((x) => x.id === gone.id);
    await api.snippetDelete(gone.id);
    snippets = await api.snippets();
    draft = null;
    selected = null;
    const next = visible[Math.min(idx, visible.length - 1)];
    if (next) pick(next);
    toast(t("sn.deleted", { t: gone.title || t("lib.untitled") }), 6000, {
      label: t("common.undo"),
      run: async () => {
        const back = await api.snippetSave(gone);
        snippets = await api.snippets();
        pick(back);
      },
    });
  }

  const abbrConflict = $derived(
    !!draft?.abbr && snippets.some((s) => s.id !== draft!.id && s.abbr && s.abbr === draft!.abbr),
  );
</script>

<div class="split">
  <section class="col-list">
    <div class="narrowsel"><SnipFolderSelect /></div>
    <div class="listhead">
      <div class="searchbox"><Icon name="search" /><input bind:value={query} placeholder={t("app.search")} spellcheck="false" /></div>
      <button class="btn icon-only" onclick={create} title={t("lib.newSnippet")} aria-label={t("lib.newSnippet")}><Icon name="plus" /></button>
    </div>
    <div class="sortbar" role="radiogroup" aria-label="sort">
      {#each [["recent", "sn.sortRecent"], ["used", "sn.sortUsed"], ["unused", "sn.sortUnused"]] as [v, k]}
        <button role="radio" aria-checked={sort === v} onclick={() => (sort = v as typeof sort)}>{t(k as Parameters<typeof t>[0])}</button>
      {/each}
    </div>
    <div class="items">
      {#each visible as s (s.id)}
        <button class="item" class:on={selected === s.id} onclick={() => pick(s)}>
          <span class="it-title">{s.title || t("lib.untitled")}</span>
          <span class="it-sub">{s.body.replace(/\s+/g, " ").slice(0, 90)}</span>
          <span class="it-meta">
            {#if uses[s.id]}<span>{t("sn.usesShort", { n: uses[s.id].count })}</span>{/if}
            {#if s.abbr}<span>{settings?.expander.prefix}{s.abbr}</span>{/if}
            {#each s.tags as tag}<span>#{tag}</span>{/each}
          </span>
        </button>
      {:else}
        <p class="empty">{query ? t("picker.empty.search", { q: query }) : t("picker.empty.snippets")}</p>
      {/each}
    </div>
  </section>

  <section class="col-edit">
    {#if draft}
      <div class="edhead">
        <input class="ed-title" bind:value={draft.title} oninput={schedule} onblur={flush} placeholder={t("lib.title")} spellcheck="false" />
        <span class="saved" class:show={Date.now() - savedAt < 2500}>{t("lib.saved")}</span>
        <button class="btn ghost" aria-label={t("lib.delete")} title={t("lib.delete")} onclick={remove}><Icon name="trash" /></button>
      </div>

      <textarea class="ed-body" class:code={looksLikeCode(draft.body)} bind:value={draft.body} oninput={schedule} onblur={flush} placeholder={t("lib.body")} spellcheck="false"></textarea>
      <p class="help">{t("lib.varsHelp")}</p>

      <div class="props">
        <label>
          <span>{t("lib.abbr")}</span>
          <div class="abbrfield">
            <span class="prefix">{settings?.expander.prefix}</span>
            <input class="field" bind:value={draft.abbr} oninput={schedule} onblur={flush} spellcheck="false" />
          </div>
          {#if abbrConflict}<small class="warn">{t("lib.abbrTaken")}</small>
          {:else if draft.abbr}<small>{t("lib.abbrHelp", { p: settings?.expander.prefix ?? "", a: draft.abbr })}</small>{/if}
        </label>
        <label>
          <span>{t("lib.folder")}</span>
          <select class="field" bind:value={draft.folder} onchange={flush}>
            <option value={null}>{t("lib.noFolder")}</option>
            {#each folders as f}<option value={f.id}>{f.name}</option>{/each}
          </select>
        </label>
        <label class="wide">
          <span>{t("lib.tags")}</span>
          <input class="field" bind:value={tagsText} oninput={schedule} onblur={flush} spellcheck="false" />
        </label>
      </div>
      <p class="stamp faint">{ago(draft.updated)} · {uses[draft.id] ? t("sn.uses", { n: uses[draft.id].count, t: ago(uses[draft.id].last) }) : t("sn.neverUsed")}</p>
    {:else}
      <p class="empty center">{t("lib.selectSnippet")}</p>
    {/if}
  </section>
</div>

<style>
  .split { display: grid; grid-template-columns: 300px minmax(0, 1fr); height: 100%; min-height: 0; }
  .narrowsel { padding: 12px 12px 0; display: none; }
  @media (max-width: 980px) { .narrowsel { display: block; } }
  .col-list { border-right: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; }
  .listhead { display: flex; gap: 8px; padding: 12px 12px 8px; }
  .icon-only { padding: 0 8px; }
  .searchbox { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; padding: 0 8px; border: 1px solid var(--line); border-radius: var(--r-s); color: var(--faint); background: var(--sunk); }
  .searchbox input { flex: 1; min-width: 0; background: none; border: 0; height: 28px; user-select: text; }
  .sortbar { display: flex; gap: 2px; padding: 0 12px 8px; border-bottom: 1px solid var(--line); }
  .sortbar button { font-size: var(--t-xs); color: var(--faint); padding: 3px 8px; border-radius: var(--r-s); }
  .sortbar button:hover { color: var(--text); }
  .sortbar button[aria-checked="true"] { color: var(--text); background: var(--raise); }
  .items { overflow-y: auto; padding: 6px; flex: 1; min-height: 0; }
  .item { width: 100%; min-width: 0; display: grid; grid-template-columns: minmax(0, 1fr); gap: 2px; text-align: left; padding: 9px 10px; border-radius: var(--r-s); content-visibility: auto; contain-intrinsic-size: auto 52px; }
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
  .saved { font-size: var(--t-xs); color: var(--faint); opacity: 0; transition: opacity 200ms var(--ease); }
  .saved.show { opacity: 1; }
  .ed-body {
    min-height: 200px; resize: vertical; padding: 12px 14px; line-height: 1.6;
    background: var(--sunk); border: 1px solid var(--line); border-radius: var(--r-s); user-select: text;
  }
  .ed-body:focus { border-color: var(--muted); }
  .ed-body.code { font-family: var(--mono); font-size: 12.5px; }
  .help { margin: 0; font-size: var(--t-s); color: var(--faint); max-width: 70ch; }
  .props { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 14px; margin-top: 12px; }
  .props label { display: grid; gap: 5px; font-size: var(--t-s); color: var(--muted); align-content: start; }
  .props .wide { grid-column: 1 / -1; }
  .props small { color: var(--faint); font-size: var(--t-xs); }
  .props small.warn { color: var(--danger); }
  .abbrfield { display: flex; align-items: center; }
  .prefix { padding: 0 8px; height: 35px; display: grid; place-items: center; border: 1px solid var(--line); border-right: 0; border-radius: var(--r-s) 0 0 var(--r-s); color: var(--muted); background: var(--raise); }
  .abbrfield .field { border-radius: 0 var(--r-s) var(--r-s) 0; }
  .stamp { font-size: var(--t-xs); margin: 8px 0 0; }
  @media (max-width: 980px) {
    .split { grid-template-columns: 230px minmax(0, 1fr); }
  }
  .empty { color: var(--muted); padding: 20px 10px; margin: 0; }
  .empty.center { margin: auto; text-align: center; }
</style>
