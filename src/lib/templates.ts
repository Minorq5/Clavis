// Встроенные шаблоны заметок. Срок в шаблонах ставится относительно сегодняшнего дня.
import { i18n } from "./i18n.svelte";

const h = (level: number, text: string) => ({ type: "heading", attrs: { level }, content: [{ type: "text", text }] });
const p = (text = "") => (text ? { type: "paragraph", content: [{ type: "text", text }] } : { type: "paragraph" });
const bold = (label: string, rest = "") => ({ type: "paragraph", content: [{ type: "text", marks: [{ type: "bold" }], text: label }, ...(rest ? [{ type: "text", text: " " + rest }] : [])] });
const ul = (items: string[]) => ({ type: "bulletList", content: items.map((t) => ({ type: "listItem", content: [p(t)] })) });
const tasks = (items: string[]) => ({ type: "taskList", content: items.map((t) => ({ type: "taskItem", attrs: { checked: false }, content: [p(t)] })) });
const due = (days: number, hour = 18) => {
  const d = new Date();
  d.setDate(d.getDate() + days);
  d.setHours(hour, 0, 0, 0);
  return d.getTime();
};
const withDue = (label: string, days: number) => ({ type: "paragraph", content: [{ type: "text", marks: [{ type: "bold" }], text: label }, { type: "text", text: " " }, { type: "deadline", attrs: { at: due(days) } }] });
const doc = (...content: unknown[]) => ({ type: "doc", content });

export interface BuiltIn { id: string; title: string; doc: () => unknown }

export function builtInTemplates(): BuiltIn[] {
  const date = new Date().toLocaleDateString(i18n.lang === "ru" ? "ru-RU" : "en-GB", { day: "numeric", month: "long" });
  if (i18n.lang === "en")
    return [
      { id: "lecture", title: "Lecture notes", doc: () => doc(h(1, `Lecture, ${date}`), bold("Subject:"), bold("Topic:"), h(2, "Key terms"), p("Term — definition"), h(2, "Notes"), p(), h(2, "Questions for the seminar"), tasks([""])) },
      { id: "brief", title: "Client brief", doc: () => doc(h(1, "Brief: Kenzie"), bold("Client:", "Kenzie"), bold("Contact:"), withDue("Deadline:", 7), bold("Budget:"), h(2, "Task"), p(), h(2, "What must be done"), tasks(["", ""]), h(2, "References"), ul([""])) },
      { id: "report", title: "Report", doc: () => doc(h(1, `Report, ${date}`), h(2, "Done"), ul([""]), h(2, "In progress"), ul([""]), h(2, "Problems"), ul([""]), h(2, "Next"), tasks([""])) },
      { id: "meeting", title: "Meeting", doc: () => doc(h(1, `Meeting, ${date}`), bold("Participants:", "Oleg Jayden, Rinusik, Trivia"), h(2, "Agenda"), ul([""]), h(2, "Decisions"), ul([""]), h(2, "Who does what"), tasks(["Rinusik: "])) },
    ];
  return [
    { id: "lecture", title: "Конспект лекции", doc: () => doc(h(1, `Лекция, ${date}`), bold("Предмет:"), bold("Тема:"), h(2, "Термины"), p("Термин — определение"), h(2, "Конспект"), p(), h(2, "Вопросы к семинару"), tasks([""])) },
    { id: "brief", title: "Бриф клиента", doc: () => doc(h(1, "Бриф: Кензи"), bold("Клиент:", "Кензи"), bold("Связь:"), withDue("Срок:", 7), bold("Бюджет:"), h(2, "Задача"), p(), h(2, "Что нужно сделать"), tasks(["", ""]), h(2, "Примеры и ссылки"), ul([""])) },
    { id: "report", title: "Отчёт", doc: () => doc(h(1, `Отчёт, ${date}`), h(2, "Сделано"), ul([""]), h(2, "В работе"), ul([""]), h(2, "Проблемы"), ul([""]), h(2, "Дальше"), tasks([""])) },
    { id: "meeting", title: "Встреча", doc: () => doc(h(1, `Встреча, ${date}`), bold("Участники:", "Олег Джайден, Ринусик, Тривия"), h(2, "Повестка"), ul([""]), h(2, "Решили"), ul([""]), h(2, "Кто что делает"), tasks(["Ринусик: "])) },
  ];
}
