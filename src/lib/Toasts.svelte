<script lang="ts">
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { toasts, dismiss } from "./toast.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast" class:act={t.action} in:fly={{ y: 8, duration: 160, easing: cubicOut }} out:fly={{ y: 4, duration: 120 }}>
      <span class="txt">{t.text}</span>
      {#if t.action}
        <button onclick={() => { t.action?.run(); dismiss(t.id); }}>{t.action.label}</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; left: 50%; bottom: 20px; transform: translateX(-50%); display: grid; gap: 6px; justify-items: center; z-index: 100; pointer-events: none; }
  .toast { display: flex; align-items: center; gap: 14px; background: var(--text); color: var(--bg); padding: 7px 14px; border-radius: var(--r-m); font-size: var(--t-s); font-weight: 500; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4); max-width: min(60vw, 520px); }
  .toast.act { pointer-events: auto; padding-right: 6px; }
  .txt { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  button { flex: none; padding: 3px 10px; border-radius: var(--r-s); font-weight: 600; color: var(--bg); box-shadow: inset 0 0 0 1px rgba(21, 21, 20, 0.35); }
  button:hover { background: rgba(21, 21, 20, 0.1); }
</style>
