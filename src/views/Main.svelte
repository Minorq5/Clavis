<script lang="ts">
  import { onMount } from "svelte";
  import { api, listen, type Folder, type SharedMeta } from "../lib/api";
  import { askText } from "../lib/dialog.svelte";
  import { sharedError } from "../lib/sharedError";
  import { toast } from "../lib/toast.svelte";
  import TeamSet from "./TeamSet.svelte";
  import { t, type Key } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import Wordmark from "../lib/Wordmark.svelte";
  import Titlebar from "../lib/Titlebar.svelte";
  import Snippets from "./Snippets.svelte";
  import Notes from "./Notes.svelte";
  import SettingsView from "./SettingsView.svelte";
  import Guide from "./Guide.svelte";
  import Owner from "./Owner.svelte";
  import Toasts from "../lib/Toasts.svelte";
  import Dialogs from "../lib/Dialogs.svelte";
  import Menu from "../lib/Menu.svelte";
  import Palette from "./Palette.svelte";
  import StarPrompt from "../lib/StarPrompt.svelte";
  import { ask } from "../lib/dialog.svelte";
  import { nav, type Section } from "../lib/nav.svelte";
  import { updates, checkUpdates, installUpdate } from "../lib/updates.svelte";
  import { fade, slide } from "svelte/transition";

  let { owner = false, initial = "snippets" }: { owner?: boolean; initial?: string } = $props();

  // svelte-ignore state_referenced_locally
  nav.section = (["snippets", "notes", "settings", "guide", "owner"].includes(initial) ? initial : "snippets") as Section;
  // Следующий запуск откроет тот раздел, где вы работали.
  $effect(() => {
    const sec = nav.section;
    if (sec === "notes" || sec === "snippets") try { localStorage.setItem("clavis.section", sec); } catch {}
  });
  const section = $derived(nav.section);
  let palette = $state(false);
  let starOpen = $state(false);
  let folders = $state<Folder[]>([]);
  const folder = $derived(nav.snipFolder);
  let hotkey = $state("");
  let naming = $state<{ id: string; name: string } | null>(null);
  let sets = $state<SharedMeta[]>([]);

  async function load() {
    folders = await api.folders();
    const st = await api.settings();
    hotkey = st.hotkey;
    // Просьба о звезде: один раз, через минуту после первого запуска, чтобы не мешать знакомству.
    if (!st.starAsked) setTimeout(() => (starOpen = true), 60_000);
    await loadSets();
  }

  async function loadSets() {
    sets = (await api.sharedList().catch(() => [])).filter((s) => s.kind === "snippets");
    nav.sets = sets.map((s) => ({ id: s.id, title: s.title }));
    nav.folders = folders.map((f) => ({ id: f.id, name: f.name }));
  }

  async function newSet() {
    const title = (await askText({ title: t("sn.newSet"), text: t("sn.teamHelp"), ok: t("common.create"), placeholder: t("sn.setPh") }))?.trim();
    if (!title) return;
    try {
      const m = await api.sharedCreate(title, "snippets");
      sets = [...sets, m];
      nav.snipFolder = "team:" + m.id;
    } catch (e) {
      toast(sharedError(e), 4000);
    }
  }

  async function joinSet() {
    const link = (await askText({ title: t("sn.joinSet"), text: t("sh.joinHelp"), ok: t("sh.joinGo"), placeholder: "clavis:join/…" }))?.trim();
    if (!link) return;
    try {
      const id = await api.sharedJoin(link);
      await loadSets();
      if (sets.some((s) => s.id === id)) nav.snipFolder = "team:" + id;
      else { nav.section = "notes"; nav.target = { kind: "shared", id }; }
    } catch (e) {
      toast(sharedError(e), 4000);
    }
  }

  // Переход к сниппету из поиска: открываем его папку или командный набор.
  $effect(() => {
    const tg = nav.target;
    if (tg?.kind !== "snippet" || nav.section !== "snippets") return;
    const m = /^team:([^:]+):/.exec(tg.id);
    nav.snipFolder = m ? "team:" + m[1] : "all";
  });

  onMount(() => {
    load();
    let off: (() => void) | undefined;
    listen<string>("main:section", (s) => { nav.section = s as Section; nav.target = null; }).then((o) => (off = o));
    // Нажали на уведомление о дедлайне или упоминании: открываем эту заметку.
    listen<string>("open:note", (id) => { nav.section = "notes"; nav.target = { kind: "note", id }; });
    // Новую версию ищем через несколько секунд после запуска, чтобы не мешать старту.
    const timer = setTimeout(checkUpdates, 4000);
    return () => { off?.(); clearTimeout(timer); };
  });

  async function saveFolder() {
    if (!naming) return;
    const name = naming.name.trim();
    if (name) {
      const f = await api.folderSave({ id: naming.id, name });
      nav.folders = [...nav.folders.filter((x) => x.id !== f.id), { id: f.id, name: f.name }];
      nav.snipFolder = f.id;
    }
    naming = null;
    folders = await api.folders();
  }

  async function deleteFolder(id: string) {
    const f = folders.find((x) => x.id === id);
    if (!(await ask({ title: t("lib.deleteFolderQ", { n: f?.name ?? "" }), text: t("lib.deleteFolderText"), ok: t("lib.delete"), danger: true }))) return;
    await api.folderDelete(id);
    nav.folders = nav.folders.filter((x) => x.id !== id);
    nav.snipFolder = "all";
    folders = await api.folders();
  }

  const titles: Record<Section, Key> = { snippets: "tab.snippets", notes: "tab.notes", settings: "tab.settings", guide: "tab.guide", owner: "tab.owner" };
</script>

<div class="shell">
  <nav>
    <div class="brand" data-tauri-drag-region><Wordmark size={16} /></div>
    <button class="nav find" title="{t('search.open')}  Ctrl+K" onclick={() => (palette = true)}><Icon name="search" /><span class="nl">{t("search.open")}</span><kbd class="nl">Ctrl K</kbd></button>

    <button class="nav" class:on={section === "snippets"} title={t("tab.snippets")} onclick={() => (nav.section = "snippets")}><Icon name="snippet" /><span class="nl">{t("tab.snippets")}</span></button>
    {#if section === "snippets"}
      <div class="folders">
        <button class="sub" class:on={folder === "all"} onclick={() => (nav.snipFolder = "all")}>{t("lib.allSnippets")}</button>
        {#each folders as f (f.id)}
          {#if naming?.id === f.id}
            <input class="field rename" bind:value={naming.name} onblur={saveFolder} onkeydown={(e) => { if (e.key === "Enter") saveFolder(); if (e.key === "Escape") naming = null; }} />
          {:else}
            <div class="subrow">
              <button class="sub" class:on={folder === f.id} onclick={() => (nav.snipFolder = f.id)} ondblclick={() => (naming = { id: f.id, name: f.name })}>{f.name}</button>
              <button class="x" aria-label={t("lib.delete")} onclick={() => deleteFolder(f.id)}><Icon name="close" size={12} /></button>
            </div>
          {/if}
        {/each}
        <button class="sub" class:on={folder === "none"} onclick={() => (nav.snipFolder = "none")}>{t("lib.noFolder")}</button>
        {#if naming?.id === ""}
          <!-- svelte-ignore a11y_autofocus -->
          <input class="field rename" autofocus bind:value={naming.name} placeholder={t("lib.folderName")} onblur={saveFolder} onkeydown={(e) => { if (e.key === "Enter") saveFolder(); if (e.key === "Escape") naming = null; }} />
        {:else}
          <button class="sub add" onclick={() => (naming = { id: "", name: "" })}><Icon name="plus" size={13} />{t("lib.newFolder")}</button>
        {/if}
        <div class="subhead">{t("sn.team")}</div>
        {#each sets as st (st.id)}
          <button class="sub" class:on={folder === "team:" + st.id} onclick={() => (nav.snipFolder = "team:" + st.id)}><Icon name="users" size={12} />{st.title || t("lib.untitled")}</button>
        {/each}
        <button class="sub add" onclick={newSet}><Icon name="plus" size={13} />{t("sn.newSetShort")}</button>
        <button class="sub add" onclick={joinSet}><Icon name="link" size={13} />{t("sn.joinSet")}</button>
      </div>
    {/if}
    <button class="nav" class:on={section === "notes"} title={t("tab.notes")} onclick={() => (nav.section = "notes")}><Icon name="note" /><span class="nl">{t("tab.notes")}</span></button>

    <div class="bottom">
      <button class="nav" class:on={section === "guide"} title={t("tab.guide")} onclick={() => (nav.section = "guide")}><Icon name="guide" /><span class="nl">{t("tab.guide")}</span></button>
      <button class="nav" class:on={section === "settings"} title={t("tab.settings")} onclick={() => (nav.section = "settings")}><Icon name="settings" /><span class="nl">{t("tab.settings")}</span></button>
      {#if owner}<button class="nav" class:on={section === "owner"} title={t("tab.owner")} onclick={() => (nav.section = "owner")}><Icon name="owner" /><span class="nl">{t("tab.owner")}</span></button>{/if}
      {#if hotkey}
        <div class="hk">
          <span>{t("tab.history")}</span>
          <span class="keys">{#each hotkey.split("+") as k}<kbd>{k}</kbd>{/each}</span>
        </div>
      {/if}
    </div>
  </nav>

  <div class="main">
    <Titlebar title={t(titles[section])} />
    {#if updates.found && !updates.dismissed}
      <div class="upd" transition:slide={{ duration: 160 }}>
        {#if updates.progress !== null}
          <span>{t("upd.installing", { p: updates.progress })}</span>
          <span class="bar"><i style="width:{updates.progress}%"></i></span>
        {:else}
          <span>{t("upd.available", { v: updates.found.version })}</span>
          {#if updates.error}<span class="uerr">{t("upd.failed", { e: updates.error })}</span>{/if}
          <span class="uacts">
            <button class="btn ghost" onclick={() => (updates.dismissed = true)}>{t("upd.later")}</button>
            <button class="btn primary" onclick={installUpdate}>{t("upd.install")}</button>
          </span>
        {/if}
      </div>
    {/if}
    <div class="content">
      {#key section + folder}
        <div class="pane" in:fade={{ duration: 140 }}>
          {#if section === "snippets" && folder.startsWith("team:")}<TeamSet id={folder.slice(5)} onleft={async () => { nav.snipFolder = "all"; await loadSets(); }} />
          {:else if section === "snippets"}<Snippets {folder} />
          {:else if section === "notes"}<Notes />
          {:else if section === "settings"}<SettingsView />
          {:else if section === "guide"}<Guide />
          {:else if section === "owner"}<Owner />{/if}
        </div>
      {/key}
    </div>
  </div>
</div>
<Toasts />
<Dialogs />
<Menu />
{#if starOpen}<StarPrompt onclose={() => (starOpen = false)} />{/if}
{#if palette}<Palette {owner} onclose={() => (palette = false)} />{/if}

<svelte:window onkeydown={(e) => {
  const ctrl = e.ctrlKey || e.metaKey;
  if (ctrl && e.code === "KeyK") { e.preventDefault(); palette = !palette; return; }
  // Ctrl+F: поиск по списку текущего раздела (в тексте заметки не перехватываем).
  if (ctrl && e.code === "KeyF" && !(e.target as HTMLElement)?.closest?.(".ProseMirror")) {
    const box = [...document.querySelectorAll<HTMLInputElement>(".searchbox input")].find((x) => x.offsetParent);
    if (box) { e.preventDefault(); box.focus(); box.select(); }
  }
}} />

<style>
  .shell { display: grid; grid-template-columns: 208px 1fr; height: 100%; }
  nav { display: flex; flex-direction: column; gap: 1px; padding: 0 10px 12px; border-right: 1px solid var(--line); background: var(--sunk); min-height: 0; overflow-y: auto; }
  .brand { height: 54px; display: flex; align-items: center; padding: 0 8px; }
  .nav { display: flex; align-items: center; gap: 10px; height: 32px; padding: 0 8px; border-radius: var(--r-s); color: var(--muted); text-align: left; }
  .nav:hover { color: var(--text); background: var(--raise); }
  .nav.on { color: var(--text); background: var(--raise); box-shadow: inset 2px 0 0 var(--text); }
  .folders { display: grid; gap: 1px; padding: 2px 0 8px 26px; }
  .subrow { display: flex; align-items: center; }
  .subrow .sub { flex: 1; }
  .sub { height: 27px; padding: 0 8px; border-radius: var(--r-s); text-align: left; color: var(--muted); font-size: var(--t-s); display: flex; align-items: center; gap: 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub:hover { color: var(--text); }
  .sub.on { color: var(--text); }
  .sub.add { color: var(--faint); }
  .x { opacity: 0; color: var(--faint); padding: 4px; border-radius: var(--r-s); }
  .subrow:hover .x { opacity: 1; }
  .x:hover { color: var(--text); }
  .subhead { padding: 12px 8px 2px; font-size: var(--t-xs); color: var(--faint); }
  .rename { height: 27px; padding: 0 8px; font-size: var(--t-s); }
  .bottom { margin-top: auto; display: grid; gap: 1px; padding-top: 16px; }
  .hk { display: flex; justify-content: space-between; align-items: center; margin-top: 12px; padding: 10px 8px 0; border-top: 1px solid var(--line); font-size: var(--t-xs); color: var(--faint); }
  .keys { display: inline-flex; gap: 2px; }
  .main { display: grid; grid-template-rows: auto auto minmax(0, 1fr); min-width: 0; min-height: 0; }
  .main > .content { grid-row: 3; }
  .find { margin-bottom: 10px; color: var(--faint); box-shadow: inset 0 0 0 1px var(--line); }
  .find kbd { margin-left: auto; font: inherit; font-size: 10.5px; color: var(--faint); }
  .upd { display: flex; align-items: center; gap: 12px; padding: 8px 12px 8px 16px; border-bottom: 1px solid var(--line); background: var(--raise); font-size: var(--t-s); }
  .uacts { margin-left: auto; display: flex; gap: 4px; }
  .uacts .btn { height: 28px; }
  .uerr { color: var(--danger); font-size: var(--t-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar { flex: 1; max-width: 260px; height: 3px; background: var(--line); border-radius: 2px; overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--text); transition: width 200ms linear; }
  .content { min-height: 0; overflow: hidden; position: relative; }
  .pane { position: absolute; inset: 0; }
  .nav { transition: background 100ms var(--ease), color 100ms var(--ease); }

  /* Узкое окно: боковая панель сворачивается до значков. */
  @media (max-width: 980px) {
    .shell { grid-template-columns: 56px 1fr; }
    nav { padding: 0 8px 12px; }
    .brand { justify-content: center; padding: 0; }
    .brand :global(.mark > span) { display: none; }
    .nav { justify-content: center; padding: 0; }
    .nl, .folders, .hk { display: none; }
  }
</style>
