<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { api, type ClipItem, type NoteMeta, type SharedMeta, type Snippet, type TeamSnippet } from "../lib/api";
  import { search } from "../lib/fuzzy";
  import { t, ago, type Key } from "../lib/i18n.svelte";
  import { go, type Section } from "../lib/nav.svelte";
  import { toast } from "../lib/toast.svelte";
  import Icon from "../lib/Icon.svelte";

  let { onclose, owner = false }: { onclose: () => void; owner?: boolean } = $props();

  type Hit = { key: string; group: Key; icon: string; title: string; sub: string; run: () => void };
  let q = $state("");
  let i = $state(0);
  let input = $state<HTMLInputElement>();
  let listEl = $state<HTMLElement>();
  let history = $state<ClipItem[]>([]);
  let snippets = $state<Snippet[]>([]);
  let team = $state<TeamSnippet[]>([]);
  let notes = $state<NoteMeta[]>([]);
  let shared = $state<SharedMeta[]>([]);

  onMount(async () => {
    input?.focus();
    const [h, s, tm, n, sh] = await Promise.all([
      api.history().catch(() => []), api.snippets().catch(() => []), api.teamSnippets().catch(() => []),
      api.notes().catch(() => []), api.sharedList().catch(() => []),
    ]);
    history = h; snippets = s; team = tm; notes = n.filter((x) => !x.trashed); shared = sh;
  });

  const one = (s: string) => s.replace(/\s+/g, " ").trim();
  const sections: { id: Section; key: Key; icon: string }[] = [
    { id: "snippets", key: "tab.snippets", icon: "snippet" },
    { id: "notes", key: "tab.notes", icon: "note" },
    { id: "settings", key: "tab.settings", icon: "settings" },
    { id: "guide", key: "tab.guide", icon: "guide" },
  ];

  const hits = $derived.by((): Hit[] => {
    const out: Hit[] = [];
    const lim = q ? 6 : 4;
    const close = (f: () => void) => () => { onclose(); f(); };
    for (const n of search(notes, q, (n) => `${n.title} ${n.tags.join(" ")} ${n.text}`).slice(0, lim))
      out.push({ key: "n" + n.id, group: "tab.notes", icon: n.secret ? "lock" : "note", title: n.title || t("lib.untitled"), sub: ago(n.updated), run: close(() => go("notes", { kind: "note", id: n.id })) });
    for (const s of search(shared, q, (s) => s.title).filter((s) => s.kind === "note").slice(0, lim))
      out.push({ key: "s" + s.id, group: "tab.notes", icon: "users", title: s.title || t("lib.untitled"), sub: ago(s.updated), run: close(() => go("notes", { kind: "shared", id: s.id })) });
    for (const s of search(snippets, q, (s) => `${s.title} ${s.abbr} ${s.tags.join(" ")} ${s.body}`).slice(0, lim))
      out.push({ key: "p" + s.id, group: "tab.snippets", icon: "snippet", title: s.title || t("lib.untitled"), sub: one(s.body).slice(0, 70), run: close(() => go("snippets", { kind: "snippet", id: s.id })) });
    for (const s of search(team, q, (s) => `${s.title} ${s.abbr} ${s.body}`).slice(0, lim))
      out.push({ key: "t" + s.setId + s.id, group: "tab.snippets", icon: "users", title: s.title, sub: s.setTitle, run: close(() => go("snippets", { kind: "snippet", id: `team:${s.setId}:${s.id}` })) });
    for (const h of search(history.filter((h) => h.kind === "text"), q, (h) => h.text ?? "").slice(0, lim))
      out.push({ key: "h" + h.id, group: "tab.history", icon: "history", title: one(h.text ?? "").slice(0, 90), sub: t("search.copy"), run: close(async () => { await api.copyText(h.text ?? ""); toast(t("common.copied")); }) });
    const secs = sections.concat(owner ? [{ id: "owner", key: "tab.owner", icon: "owner" }] : []);
    for (const s of search(secs, q, (s) => t(s.key)))
      out.push({ key: "x" + s.id, group: "search.sections", icon: s.icon, title: t(s.key), sub: "", run: close(() => go(s.id)) });
    return out;
  });

  $effect(() => { q; i = 0; });

  async function move(d: number) {
    if (!hits.length) return;
    i = (i + d + hits.length) % hits.length;
    await tick();
    listEl?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onclose(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter") { e.preventDefault(); hits[i]?.run(); }
  }
</script>

<div class="back" transition:fade={{ duration: 100 }} onclick={onclose} role="presentation"></div>
<div class="pal" role="dialog" aria-modal="true" aria-label={t("search.ph")} transition:scale={{ start: 0.98, duration: 140, easing: cubicOut }}>
  <label class="in">
    <Icon name="search" />
    <input bind:this={input} bind:value={q} placeholder={t("search.ph")} spellcheck="false" onkeydown={onkey} />
    <kbd>Esc</kbd>
  </label>
  <div class="list" bind:this={listEl} role="listbox">
    {#each hits as h, n (h.key)}
      {#if n === 0 || hits[n - 1].group !== h.group}<div class="grp">{t(h.group)}</div>{/if}
      <button role="option" aria-selected={n === i} onmousemove={() => (i = n)} onclick={h.run}>
        <Icon name={h.icon} size={14} />
        <span class="tt">{h.title}</span>
        <span class="sb">{h.sub}</span>
      </button>
    {:else}
      <p class="none">{t("search.empty")}</p>
    {/each}
  </div>
</div>

<style>
  .back { position: fixed; inset: 0; background: rgba(8, 8, 8, 0.55); z-index: 60; }
  .pal { position: fixed; left: 50%; top: 12vh; translate: -50% 0; width: min(620px, calc(100vw - 32px)); max-height: 70vh; z-index: 61; display: grid; grid-template-rows: auto minmax(0, 1fr); background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-m); box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55); overflow: hidden; }
  .in { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--line); color: var(--faint); }
  .in input { flex: 1; min-width: 0; background: none; border: 0; font-size: var(--t-l); color: var(--text); user-select: text; }
  .in input:focus-visible { outline: none; }
  kbd { font: inherit; font-size: var(--t-xs); color: var(--faint); }
  .list { overflow-y: auto; padding: 6px; }
  .grp { padding: 10px 10px 4px; font-size: var(--t-xs); color: var(--faint); }
  button { width: 100%; display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px; border-radius: var(--r-s); text-align: left; color: var(--muted); }
  button[aria-selected="true"] { background: var(--text); color: var(--bg); }
  .tt { flex: 1; min-width: 0; color: inherit; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button:not([aria-selected="true"]) .tt { color: var(--text); }
  .sb { flex: none; max-width: 40%; font-size: var(--t-xs); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; opacity: 0.75; }
  .none { color: var(--muted); padding: 22px 12px; margin: 0; }
</style>
