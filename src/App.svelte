<script lang="ts">
  import { onMount } from "svelte";
  import { api, listen, type Status, type PickerMode } from "./lib/api";
  import { i18n } from "./lib/i18n.svelte";
  import Picker from "./views/Picker.svelte";
  import Main from "./views/Main.svelte";
  import Setup from "./views/Setup.svelte";
  import Lock from "./views/Lock.svelte";
  import Print from "./views/Print.svelte";
  import Consent from "./views/Consent.svelte";
  import Quick from "./views/Quick.svelte";
  import { applyTheme } from "./lib/theme";
  import { LEGAL_VERSION } from "./lib/legal";
  import type { Settings } from "./lib/api";

  // Одно приложение обслуживает несколько окон. Какое это окно, говорит адрес.
  const hash = new URLSearchParams(location.hash.slice(1));
  const view = hash.get("view") ?? "main";
  const mode = (hash.get("mode") ?? "popup") as PickerMode;
  const section = hash.get("section") ?? "snippets";

  let status = $state<Status | null>(null);
  let settings = $state<Settings | null>(null);
  // Согласие с условиями спрашивается до любых действий, а после новой редакции документов — снова.
  const needConsent = $derived(!!settings && view !== "picker" && view !== "quick" && settings.legalAccepted !== LEGAL_VERSION);

  async function acceptLegal(version: string) {
    if (!settings) return;
    const next = { ...$state.snapshot(settings), legalAccepted: version, lang: i18n.lang };
    await api.settingsSave(next).catch(() => {});
    settings = next;
  }

  async function refresh() {
    status = await api.status();
    const s = await api.settings().catch(() => null);
    if (s) { i18n.lang = s.lang; settings = s; applyTheme(s.theme); }
  }

  onMount(() => {
    if (view === "print") return;
    refresh();
    let off: (() => void) | undefined;
    listen("status:changed", refresh).then((o) => (off = o));
    listen("settings:changed", refresh);
    return () => off?.();
  });
</script>

{#if view === "print"}
  <Print />
{:else if needConsent}
  <Consent onaccept={acceptLegal} updated={!!settings?.legalAccepted} />
{:else if status}
  {#if view === "picker"}
    {#if status.stage === "ready"}<Picker initialMode={mode} />{/if}
  {:else if view === "quick"}
    <Quick />
  {:else if status.stage === "activate" || status.stage === "setup"}
    <Setup stage={status.stage} ondone={refresh} />
  {:else if status.stage === "locked" || status.stage === "blocked"}
    <Lock stage={status.stage} ondone={refresh} />
  {:else}
    <Main owner={status.owner} initial={section} />
  {/if}
{/if}
