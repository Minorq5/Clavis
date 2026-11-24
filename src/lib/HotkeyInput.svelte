<script lang="ts">
  import { t } from "./i18n.svelte";

  let { value = $bindable(""), onchange }: { value: string; onchange?: (v: string) => void } = $props();
  let recording = $state(false);

  function keyName(code: string): string | null {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3);
    if (/^Digit\d$/.test(code)) return code.slice(5);
    if (/^F\d{1,2}$/.test(code)) return code;
    const map: Record<string, string> = { Space: "Space", Backquote: "`", Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown", Period: ".", Comma: ",", Slash: "/", Semicolon: ";", BracketLeft: "[", BracketRight: "]" };
    return map[code] ?? null;
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") { recording = false; return; }
    const k = keyName(e.code);
    if (!k) return;
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
    if (!mods.length && !/^F\d/.test(k)) return; // без модификатора допустимы только F-клавиши
    value = [...mods, k].join("+");
    recording = false;
    onchange?.(value);
  }
</script>

<button
  class="hk"
  class:rec={recording}
  onclick={() => (recording = true)}
  onblur={() => (recording = false)}
  {onkeydown}
>
  {#if recording}
    <span class="faint">{t("set.hotkeyWait")}</span>
  {:else}
    {#each value.split("+") as part, i}{#if i}<span class="plus">+</span>{/if}<kbd>{part}</kbd>{/each}
  {/if}
</button>

<style>
  .hk { display: inline-flex; align-items: center; gap: 3px; min-width: 180px; height: 34px; padding: 0 10px; border: 1px solid var(--line); border-radius: var(--r-s); background: var(--sunk); }
  .hk:hover { border-color: var(--line-strong); }
  .hk.rec { border-color: var(--text); }
  .plus { color: var(--faint); font-size: var(--t-xs); }
  kbd { color: var(--text); }
</style>
