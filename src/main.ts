import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

// Контекстное меню браузера в приложении не нужно, кроме полей ввода.
window.addEventListener("contextmenu", (e) => {
  const el = e.target as HTMLElement;
  if (!el.closest("input, textarea, [contenteditable='true']")) e.preventDefault();
});

export default mount(App, { target: document.getElementById("app")! });
