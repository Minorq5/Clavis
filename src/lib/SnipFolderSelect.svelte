<script lang="ts">
  // Выбор папки сниппетов в узком окне, когда боковая панель свёрнута до значков.
  import { nav } from "./nav.svelte";
  import { t } from "./i18n.svelte";
</script>

<select class="field fsel" bind:value={nav.snipFolder} aria-label={t("lib.folder")}>
  <option value="all">{t("lib.allSnippets")}</option>
  {#each nav.folders as f (f.id)}<option value={f.id}>{f.name}</option>{/each}
  <option value="none">{t("lib.noFolder")}</option>
  {#if nav.sets.length}
    <optgroup label={t("sn.team")}>
      {#each nav.sets as s (s.id)}<option value={"team:" + s.id}>{s.title || t("lib.untitled")}</option>{/each}
    </optgroup>
  {/if}
</select>

<style>
  .fsel { display: none; height: 30px; padding: 0 8px; font-size: var(--t-s); }
  @media (max-width: 980px) { .fsel { display: block; } }
</style>
