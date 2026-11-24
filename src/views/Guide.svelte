<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { i18n } from "../lib/i18n.svelte";
  import Figure from "../lib/Figure.svelte";

  let hotkey = $state("Ctrl+Shift+V");
  let prefix = $state(";");
  onMount(async () => {
    const s = await api.settings();
    hotkey = s.hotkey;
    prefix = s.expander.prefix;
  });
  const ru = $derived(i18n.lang === "ru");
  const L = (r: string, e: string) => (ru ? r : e);
  const keys = (k: string) => k.split("+");
</script>

{#snippet combo(k: string)}
  <span class="combo">{#each keys(k) as p, i}{#if i}<span class="plus">+</span>{/if}<kbd>{p}</kbd>{/each}</span>
{/snippet}

{#snippet row(text: string, time: string, sel = false, icon = "t")}
  <div class="m-row" class:sel><span class="m-ic {icon}"></span><span class="m-tx">{text}</span><span class="m-tm">{time}</span></div>
{/snippet}

<div class="page">
  <h1>{L("Как пользоваться Clavis", "How to use Clavis")}</h1>
  <p class="lead">{L(
    "Clavis помнит, что вы скопировали, и хранит то, что вы решили оставить. Схемы ниже показывают всё, что нужно знать.",
    "Clavis remembers what you copied and keeps what you decided to save. The diagrams below show everything you need.",
  )}</p>

  <section>
    <h2>{L("Вызвать из любой программы", "Call it from any program")}</h2>
    <p class="text">{L(
      "Пишете письмо, заполняете форму, отвечаете в мессенджере: нажмите сочетание, и Clavis откроется рядом с текстовым курсором. Выбранное вставится туда же.",
      "Writing an email, filling a form, replying in a chat: press the shortcut and Clavis opens next to the text cursor. The selection is pasted right there.",
    )}</p>
    <Figure
      w={640}
      h={250}
      label={L("Схема: окно Clavis открывается у курсора", "Diagram: Clavis opens at the cursor")}
      callouts={[
        { text: L("Нажмите в любой программе", "Press in any program"), at: [470, 92], to: [452, 52], width: 150, from: "top", bend: -0.2 },
        { text: L("Окно появляется там, где вы печатаете", "The window appears where you type"), at: [470, 170], to: [390, 150], width: 150, from: "left", bend: 0.1 },
      ]}
    >
      <div class="m-win" style="left:24px;top:24px;width:380px;height:204px">
        <div class="m-bar"><span>{L("Новое письмо", "New email")}</span></div>
        <div class="m-doc">
          <p>{L("Здравствуйте, Ринусик!", "Hello, Rinusik!")}</p>
          <p>{L("Адрес доставки:", "Delivery address:")} <span class="m-caret"></span></p>
        </div>
      </div>
      <div class="m-pop" style="left:150px;top:104px;width:236px">
        <div class="m-search">{L("ул", "st")}<span class="m-caret small"></span></div>
        {@render row(L("ул. Садовая, 14, кв. 3", "14 Garden St, apt 3"), L("2 мин", "2 min"), true)}
        {@render row(L("Улучшить контраст", "Improve contrast"), L("1 ч", "1 h"))}
      </div>
      <div class="m-keys" style="left:430px;top:28px">{@render combo(hotkey)}</div>
    </Figure>
  </section>

  <section>
    <h2>{L("Окно Clavis", "The Clavis window")}</h2>
    <p class="text">{L(
      "Слева то, что вы копировали, справа полный текст выбранной записи. Поиск прощает опечатки и неверную раскладку: «ghbdtn» найдёт «привет».",
      "On the left is what you copied, on the right the full text of the selection. Search forgives typos and the wrong keyboard layout.",
    )}</p>
    <Figure
      w={640}
      h={300}
      label={L("Схема: части окна Clavis", "Diagram: parts of the Clavis window")}
      callouts={[
        { text: L("Печатайте, чтобы найти", "Type to search"), at: [8, 38], to: [156, 48], width: 112 },
        { text: L("Стрелки выбирают, Enter вставляет", "Arrows pick, Enter pastes"), at: [8, 118], to: [156, 125], width: 112 },
        { text: L("Tab переключает на сниппеты", "Tab switches to snippets"), at: [520, 30], to: [494, 42], width: 112 },
        { text: L("Ctrl+T исправит текст перед вставкой", "Ctrl+T fixes text before pasting"), at: [520, 128], to: [404, 244], width: 112, bend: -0.18 },
        { text: L("Ctrl+P закрепит навсегда", "Ctrl+P pins forever"), at: [520, 214], to: [466, 244], width: 112, bend: 0.1 },
      ]}
    >
      <div class="m-pop full" style="left:150px;top:28px;width:350px;height:238px">
        <div class="m-head">
          <div class="m-search">{L("ул", "st")}<span class="m-caret small"></span></div>
          <div class="m-tabs"><span class="on">{L("История", "History")}</span><span>{L("Сниппеты", "Snippets")}</span></div>
        </div>
        <div class="m-split">
          <div class="m-list">
            <div class="m-group">{L("Закреплено", "Pinned")}</div>
            {@render row(L("Статья 45. Договор…", "Article 45. Contract…"), L("38 мин", "38 min"))}
            <div class="m-group">{L("Недавнее", "Recent")}</div>
            {@render row(L("ул. Садовая, 14", "14 Garden St"), L("2 мин", "2 min"), true)}
            {@render row(L("Улучшить контраст", "Improve contrast"), L("1 ч", "1 h"))}
            {@render row("banner_v3.png", L("3 ч", "3 h"), false, "f")}
          </div>
          <div class="m-prev">
            <p>{L("ул. Садовая, 14, кв. 3, подъезд 2", "14 Garden St, apt 3, entrance 2")}</p>
            <div class="m-actions"><span class="m-btn on">{L("Изменить", "Change")}</span><span class="m-btn">{L("Закрепить", "Pin")}</span></div>
          </div>
        </div>
      </div>
    </Figure>
  </section>

  <section>
    <h2>{L("Действия с записью", "Actions on an entry")}</h2>
    <p class="text">{L(
      "Выберите запись и нажмите Ctrl+T или стрелку вправо. Clavis предложит то, что с ней можно сделать: посчитать выражение, перевести, сократить ссылку, сделать QR-код, а из картинки достать текст. Enter вставляет результат, Esc возвращает назад.",
      "Select an entry and press Ctrl+T or the right arrow. Clavis offers what can be done with it: calculate, translate, shorten a link, make a QR code, or pull the text out of an image. Enter pastes the result, Esc goes back.",
    )}</p>
    <div class="md">
      <div class="md-row"><code>200 + 15%</code><span class="md-to"></span><span class="md-h small">= 230</span></div>
      <div class="md-row"><code>{L("Здравствуйте, Тривия", "Hello, Trivia")}</code><span class="md-to"></span><span class="md-li plain">{L("Hello, Trivia", "Здравейте, Тривия")}</span></div>
      <div class="md-row"><code>https://…/very/long/link</code><span class="md-to"></span><span class="md-li plain">clck.ru/3Fk</span></div>
      <div class="md-row"><code>{L("снимок экрана", "screenshot")}</code><span class="md-to"></span><span class="md-li plain">{L("текст с картинки", "text from the image")}</span></div>
    </div>
    <p class="text small">{L(
      "Перевод и короткие ссылки отправляют текст во внешние службы без шифрования. Поэтому в первый раз Clavis спросит разрешения. Отозвать его можно в настройках.",
      "Translation and short links send text to outside services without encryption, so Clavis asks the first time. You can revoke it in settings.",
    )}</p>
  </section>

  <section>
    <h2>{L("Поиск везде", "Search everything")}</h2>
    <p class="text">{L(
      "Ctrl+K в главном окне ищет сразу по заметкам, сниппетам и истории. Enter открывает найденное, а запись истории копирует.",
      "Ctrl+K in the main window searches notes, snippets and history at once. Enter opens the result; a history entry is copied.",
    )}</p>
  </section>

  <section>
    <h2>{L("Сокращения", "Abbreviations")}</h2>
    <p class="text">{L(
      `Дайте сниппету короткое имя и набирайте его со знаком «${prefix}» в любой программе. После пробела сокращение заменится полным текстом.`,
      `Give a snippet a short name and type it with “${prefix}” in any program. After a space it turns into the full text.`,
    )}</p>
    <Figure
      w={640}
      h={176}
      label={L("Схема: сокращение превращается в текст", "Diagram: an abbreviation turns into text")}
      callouts={[]}
    >
      <div class="m-card" style="left:24px;top:40px;width:240px">
        <span class="m-cap">{L("Набираете", "You type")}</span>
        <p>{L("Спасибо за заказ!", "Thanks for the order!")}<br />{prefix}{L("подп", "sig")}<span class="m-caret"></span></p>
      </div>
      <div class="m-keys" style="left:284px;top:62px"><kbd class="space">{L("Пробел", "Space")}</kbd></div>
      <div class="m-arrow" style="left:300px;top:100px;width:52px"></div>
      <div class="m-card on" style="left:372px;top:28px;width:244px">
        <span class="m-cap">{L("Получаете", "You get")}</span>
        <p>{L("Спасибо за заказ!", "Thanks for the order!")}<br />{L("Кензи", "Kenzie")}<br /><span class="m-dim">{L("веб-дизайн и разработка", "web design and development")}</span></p>
      </div>
    </Figure>
  </section>

  <section>
    <h2>{L("Поля в сниппетах", "Fields in snippets")}</h2>
    <p class="text">{L(
      "Слово в фигурных скобках становится полем. Clavis спросит его при вставке. {дата}, {время} и {буфер} заполняются сами.",
      "A word in braces becomes a field you fill in when pasting. {date}, {time} and {clipboard} fill in by themselves.",
    )}</p>
    <Figure
      w={640}
      h={196}
      label={L("Схема: заполнение полей сниппета", "Diagram: filling snippet fields")}
      callouts={[]}
    >
      <div class="m-card" style="left:16px;top:44px;width:180px">
        <span class="m-cap">{L("Сниппет", "Snippet")}</span>
        <p class="mono">{L("Здравствуйте,", "Hello,")} <mark>{L("{имя}", "{name}")}</mark>! {L("Оценку пришлю до", "Estimate by")} <mark>{L("{срок}", "{due}")}</mark>.</p>
      </div>
      <div class="m-arrow" style="left:204px;top:96px"></div>
      <div class="m-card" style="left:236px;top:30px;width:170px">
        <span class="m-cap">{L("Clavis спрашивает", "Clavis asks")}</span>
        <div class="m-field"><small>{L("имя", "name")}</small><span>{L("Ринусик", "Rinusik")}</span></div>
        <div class="m-field"><small>{L("срок", "due")}</small><span>{L("пятницы", "Friday")}</span></div>
      </div>
      <div class="m-arrow" style="left:414px;top:96px"></div>
      <div class="m-card on" style="left:446px;top:44px;width:178px">
        <span class="m-cap">{L("Вставляется", "Pasted")}</span>
        <p>{L("Здравствуйте, Ринусик! Оценку пришлю до пятницы.", "Hello, Rinusik! Estimate by Friday.")}</p>
      </div>
    </Figure>
  </section>

  <section>
    <h2>{L("Заметки", "Notes")}</h2>
    <p class="text">{L(
      "Оформление набирается прямо в тексте, без мыши. Косая черта в начале строки открывает меню всех блоков. Изображение можно вставить из буфера или перетащить, а готовую заметку сохранить как документ Word.",
      "Formatting is typed right in the text. A slash at the start of a line opens the block menu. Paste or drop images, and export a note to Word.",
    )}</p>
    <div class="md">
      <div class="md-row"><code># {L("Лекция 3", "Lecture 3")}</code><span class="md-to"></span><span class="md-h">{L("Лекция 3", "Lecture 3")}</span></div>
      <div class="md-row"><code>- {L("суверенитет", "sovereignty")}</code><span class="md-to"></span><span class="md-li">{L("суверенитет", "sovereignty")}</span></div>
      <div class="md-row"><code>[ ] {L("выписать определения", "write out definitions")}</code><span class="md-to"></span><span class="md-task"><i></i>{L("выписать определения", "write out definitions")}</span></div>
      <div class="md-row"><code>&gt; {L("цитата", "quote")}</code><span class="md-to"></span><span class="md-q">{L("цитата", "quote")}</span></div>
      <div class="md-row"><code>```</code><span class="md-to"></span><span class="md-code">{L("блок кода", "code block")}</span></div>
      <div class="md-row"><code>@30.11-18:00</code><span class="md-to"></span><span class="md-due">{L("30 нояб. 18:00", "30 Nov 18:00")}</span></div>
    </div>
    <p class="text">{L(
      "Срок пишется через «@»: @30.11, @18:00, @завтра. В назначенное время Windows напомнит о нём, а ближайшие сроки видны над списком заметок. Отмеченная задача больше не напоминает.",
      "A deadline is written with “@”: @30.11, @18:00, @tomorrow. Windows reminds you at that time, and upcoming deadlines are shown above the note list. A checked task no longer reminds.",
    )}</p>
    <p class="text">{L(
      "Плюс над списком создаёт заметку: пустую или по шаблону. Ctrl+N делает пустую сразу. Папки выбираются над списком, теги добавляются над текстом. Поиск по тегу начинается с решётки: #право.",
      "The plus above the list creates a note, blank or from a template. Ctrl+N makes a blank one at once. Folders are chosen above the list, tags are added above the text. To search by tag, start with a hash: #law.",
    )}</p>
    <p class="text">{L(
      "Навели на заметку: справа появятся «закрепить» и «в корзину». Правая кнопка мыши открывает остальное. Удалённое лежит в корзине 30 дней, а сразу после удаления его можно вернуть кнопкой «Отменить».",
      "Hover a note to see pin and trash on the right. Right-click for the rest. Deleted notes stay in the trash for 30 days, and right after deleting you can bring one back with Undo.",
    )}</p>
    <p class="text">{L(
      "В меню «Ещё» (три точки) есть история версий, экспорт в Word, PDF и Markdown, сохранение шаблона и пароль на заметку. Секретная заметка шифруется отдельным паролем и не попадает в поиск. Забытый пароль не восстановит никто, даже владелец Clavis.",
      "The More menu (three dots) has version history, export to Word, PDF and Markdown, saving a template and a password for the note. A secret note is encrypted with its own password and is not searchable. No one can recover a forgotten password, not even the owner of Clavis.",
    )}</p>
  </section>

  <section>
    <h2>{L("Общие заметки", "Shared notes")}</h2>
    <p class="text">{L(
      "Создайте общую заметку в разделе «Заметки» или сделайте общей свою. Кнопка «Пригласить» даёт одноразовую ссылку: отправьте её человеку с Clavis, он откроет её через «Открыть по ссылке». Вы видите курсоры друг друга и правки в ту же секунду.",
      "Create a shared note in Notes or make one of yours shared. Invite gives a one-time link: send it to someone with Clavis, and they open it via Open by link. You see each other's cursors and edits instantly.",
    )}</p>
    <Figure
      w={640}
      h={232}
      label={L("Схема: совместное редактирование со сквозным шифрованием", "Diagram: end-to-end encrypted co-editing")}
      callouts={[
        { text: L("Курсор соавтора с его именем", "A co-author's cursor with their name"), at: [96, 192], to: [158, 112], width: 160, from: "top", bend: 0.08 },
        { text: L("Сервер видит только шифр", "The server sees only ciphertext"), at: [252, 186], to: [320, 140], width: 140, from: "top", bend: 0 },
      ]}
    >
      <div class="m-win" style="left:16px;top:22px;width:210px;height:156px">
        <div class="m-bar"><span>{L("Олег Джайден", "Oleg Jayden")}</span></div>
        <div class="m-doc small">
          <p class="h">{L("План лекции", "Lecture plan")}</p>
          <p>{L("1. Суверенитет", "1. Sovereignty")}<span class="m-caret"></span></p>
          <p>{L("2. Формы правления", "2. Forms of rule")}<span class="m-peer end" style="--c:#7cc4bf"><b>{L("Тривия", "Trivia")}</b></span></p>
        </div>
      </div>
      <div class="m-cipher" style="left:262px;top:84px;width:116px">
        <span>9f3a c7e1 04b8</span><span>e2d9 51aa 7c30</span>
      </div>
      <div class="m-arrow" style="left:232px;top:110px;width:26px"></div>
      <div class="m-arrow" style="left:384px;top:110px;width:26px"></div>
      <div class="m-win" style="left:414px;top:22px;width:210px;height:156px">
        <div class="m-bar"><span>{L("Тривия", "Trivia")}</span></div>
        <div class="m-doc small">
          <p class="h">{L("План лекции", "Lecture plan")}</p>
          <p>{L("1. Суверенитет", "1. Sovereignty")}<span class="m-caret"></span></p>
          <p>{L("2. Формы правления", "2. Forms of rule")}<span class="m-peer end" style="--c:#d9a46c"><b>{L("Олег Джайден", "Oleg Jayden")}</b></span></p>
        </div>
      </div>
    </Figure>
    <div class="facts">
      <p>{L(
        "Текст, курсоры и изображения шифруются на вашем компьютере ключом заметки. Ключ есть только у участников, поэтому сервер хранит и пересылает данные, но прочитать их не может.",
        "Text, cursors and images are encrypted on your computer with the note key. Only members have the key, so the server stores and relays data but cannot read it.",
      )}</p>
      <p>{L(
        "Владелец может удалить участника. Тогда ключ заметки меняется, и удалённый больше не увидит новых правок.",
        "The owner can remove a member. The note key then changes, and the removed person no longer sees new edits.",
      )}</p>
      <p>{L(
        "Без интернета заметку можно править дальше: правки сохранятся и уйдут, когда связь вернётся.",
        "Without internet you can keep editing: edits are saved and sent when the connection returns.",
      )}</p>
    </div>
  </section>

  <section>
    <h2>{L("Быстрая заметка, ссылки и учёт времени", "Quick note, links and time tracking")}</h2>
    <p class="text">{L(
      "Ctrl+Alt+N в любой программе открывает маленькое окно: напишите мысль и нажмите Enter, она станет заметкой. Сочетание меняется в настройках.",
      "Ctrl+Alt+N in any program opens a small window: write a thought and press Enter, and it becomes a note. The shortcut can be changed in settings.",
    )}</p>
    <div class="md">
      <div class="md-row"><code>[[{L("Лекция 3", "Lecture 3")}</code><span class="md-to"></span><span class="md-li plain"><u>{L("Лекция 3", "Lecture 3")}</u></span></div>
      <div class="md-row"><code>/{L("Учёт времени", "Time tracking")}</code><span class="md-to"></span><span class="md-li plain">{L("Начать — Закончить, итого 1 ч 25 мин", "Start — Stop, total 1h 25m")}</span></div>
    </div>
    <p class="text">{L(
      "Двойная квадратная скобка ищет заметку по названию и вставляет ссылку на неё. Учёт времени считает часы по клиенту: нажали «Начать», поработали, «Закончить», внизу итог для счёта. Картинку из истории буфера можно вставить в заметку через кнопку изображения.",
      "A double square bracket finds a note by title and inserts a link to it. Time tracking counts hours per client: Start, work, Stop, and the total for the invoice is below. An image from clipboard history can be inserted into a note with the image button.",
    )}</p>
    <p class="text">{L(
      "В «Настройках» есть светлая тема и резервная копия: все данные одним файлом под отдельным паролем. Положите его на флешку, и при поломке компьютера ничего не пропадёт.",
      "Settings have a light theme and a backup: all data in one file under its own password. Keep it on a USB drive and nothing is lost if the computer breaks.",
    )}</p>
  </section>

  <section>
    <h2>{L("Комментарии и командные сниппеты", "Comments and team snippets")}</h2>
    <p class="text">{L(
      "В общей заметке выделите текст и нажмите значок комментария на панели. Соавторы увидят пометку, ответят, а решённое уйдёт из текста.",
      "In a shared note, select text and press the comment icon on the toolbar. Co-authors see the mark and reply; resolved comments leave the text.",
    )}</p>
    <p class="text">{L(
      "Командный набор живёт в разделе «Сниппеты». Его правят все участники, а сокращения из него работают у каждого: ответы клиентам, фразы для RP, всё, что команда пишет одинаково. Ринусик добавил сниппет, и через секунду он есть у Кензи. Напишите в комментарии «@Тривия», и Тривия получит уведомление.",
      "A team set lives under Snippets. Every member can edit it, and its abbreviations work for everyone: client replies, RP phrases, whatever the team writes the same way. Rinusik adds a snippet, and a second later Kenzie has it. Write “@Trivia” in a comment and Trivia gets a notification.",
    )}</p>
  </section>

  <section>
    <h2>{L("Что происходит с данными", "What happens to your data")}</h2>
    <div class="facts">
      <p>{L(
        "Всё хранится на этом компьютере в зашифрованном виде. Ключ защищён PIN-кодом и чипом безопасности компьютера, поэтому скопированные файлы без них бесполезны.",
        "Everything is stored encrypted on this computer. The key is protected by your PIN and the computer's security chip, so copied files are useless without them.",
      )}</p>
      <p>{L(
        "То, что копируют менеджеры паролей, Clavis не запоминает. Любую программу можно исключить в настройках, а запись истории поставить на паузу.",
        "Clavis ignores what password managers copy. Any program can be excluded in settings, and recording can be paused.",
      )}</p>
      <p>{L(
        "После бездействия Clavis блокируется. То, что вы скопируете в это время, сохранится зашифрованным и появится в истории после ввода PIN.",
        "After inactivity Clavis locks. Anything you copy meanwhile is stored encrypted and appears in history once you enter the PIN.",
      )}</p>
    </div>
  </section>
</div>

<style>
  .page { overflow-y: auto; height: 100%; padding: 32px clamp(20px, 4vw, 48px) 72px; }
  .page > * { max-width: 800px; }
  h1 { font: 600 var(--t-xxl)/1.2 var(--font-display); margin: 0 0 10px; letter-spacing: -0.01em; }
  .lead { color: var(--muted); font-size: var(--t-l); max-width: 58ch; margin: 0 0 40px; line-height: 1.55; }
  section { margin-bottom: 44px; }
  h2 { font: 600 var(--t-l)/1.3 var(--font-display); margin: 0 0 6px; }
  .text { margin: 0; max-width: 64ch; line-height: 1.6; color: var(--muted); }
  .combo { display: inline-flex; align-items: center; gap: 3px; }
  .combo kbd { color: var(--text); font-size: 12px; padding: 4px 7px; }
  .plus { color: var(--faint); font-size: var(--t-xs); }

  /* Мини-макеты интерфейса внутри схем. */
  .md-h.small { font-size: 13px; font-family: var(--mono); }
  .md-li.plain::before { display: none; }
  .md-due { display: inline-flex; align-items: center; gap: 6px; padding: 1px 8px; border-radius: 4px; background: var(--raise); box-shadow: inset 0 0 0 1px var(--line-strong); font-size: 12px; }
  .md-due::before { content: ""; width: 6px; height: 6px; border-radius: 50%; background: var(--ok); }
  .text.small { font-size: var(--t-s); color: var(--faint); }
  .m-win, .m-pop, .m-card { position: absolute; background: var(--bg); border: 1px solid var(--line-strong); border-radius: 5px; overflow: hidden; }
  .m-bar { height: 24px; display: flex; align-items: center; padding: 0 10px; border-bottom: 1px solid var(--line); font-size: 10.5px; color: var(--faint); }
  .m-doc { padding: 14px 16px; font-size: 12px; line-height: 1.8; color: var(--text); }
  .m-doc p { margin: 0; }
  .m-doc.small { padding: 8px 12px; font-size: 11px; line-height: 2.35; }
  .m-doc .h { font-weight: 600; font-size: 13px; }
  .m-peer { position: relative; display: inline-block; width: 0; height: 12px; border-left: 1.5px solid var(--c); vertical-align: -2px; margin-right: 1px; }
  .m-peer b { position: absolute; left: -1.5px; top: -14px; background: var(--c); color: #151514; font-size: 8.5px; line-height: 1.35; padding: 0 4px; border-radius: 2px 2px 2px 0; white-space: nowrap; }
  .m-peer.end { margin-left: 1px; }
  .m-cipher { position: absolute; display: grid; gap: 2px; padding: 10px; border: 1px dashed var(--line-strong); border-radius: 5px; font: 10px/1.4 var(--mono); color: var(--faint); text-align: center; background: var(--bg); }
  .m-caret { display: inline-block; width: 1.5px; height: 13px; background: var(--text); vertical-align: -2px; animation: blink 1.1s steps(1) infinite; }
  .m-caret.small { height: 11px; margin-left: 1px; }
  @keyframes blink { 50% { opacity: 0; } }
  .m-pop { box-shadow: 0 14px 40px rgba(0, 0, 0, 0.55); padding: 4px; }
  .m-pop.full { padding: 0; display: grid; grid-template-rows: auto 1fr; }
  .m-search { display: flex; align-items: center; height: 26px; padding: 0 8px; font-size: 12px; color: var(--text); border-bottom: 1px solid var(--line); }
  .m-pop:not(.full) .m-search { margin: -4px -4px 4px; }
  .m-head { display: flex; align-items: center; border-bottom: 1px solid var(--line); }
  .m-head .m-search { flex: 1; border: 0; height: 32px; }
  .m-tabs { display: flex; gap: 2px; padding: 0 6px; font-size: 10px; color: var(--faint); }
  .m-tabs span { padding: 2px 6px; border-radius: 3px; }
  .m-tabs .on { color: var(--text); background: var(--raise); box-shadow: inset 0 0 0 1px var(--line); }
  .m-split { display: grid; grid-template-columns: 1.1fr 1fr; min-height: 0; }
  .m-list { padding: 4px; border-right: 1px solid var(--line); }
  .m-group { font-size: 9px; color: var(--faint); padding: 5px 6px 2px; }
  .m-row { display: flex; align-items: center; gap: 6px; height: 22px; padding: 0 6px; border-radius: 3px; font-size: 10.5px; color: var(--text); }
  .m-row.sel { background: var(--text); color: var(--bg); }
  .m-ic { width: 9px; height: 7px; flex: none; border-top: 1.3px solid currentColor; border-bottom: 1.3px solid currentColor; opacity: 0.55; }
  .m-ic.f { height: 10px; border: 1.2px solid currentColor; border-radius: 1px; }
  .m-tx { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .m-tm { font-size: 9px; opacity: 0.55; }
  .m-prev { display: grid; grid-template-rows: 1fr auto; font-size: 10.5px; line-height: 1.5; }
  .m-prev p { margin: 0; padding: 10px; }
  .m-actions { display: flex; justify-content: flex-end; gap: 4px; padding: 5px; border-top: 1px solid var(--line); }
  .m-btn { font-size: 9px; color: var(--faint); padding: 2px 6px; border-radius: 3px; }
  .m-btn.on { color: var(--text); background: var(--raise); }
  .m-keys { position: absolute; }
  .m-keys .space { display: inline-block; min-width: 80px; text-align: center; color: var(--text); font-size: 11px; padding: 5px 8px; }
  .m-card { padding: 10px 12px; }
  .m-card.on { border-color: var(--text); }
  .m-cap { display: block; font-size: 10px; color: var(--faint); margin-bottom: 6px; }
  .m-card p { margin: 0; font-size: 12px; line-height: 1.6; }
  .m-card p.mono { font-family: var(--mono); font-size: 10.5px; }
  .m-card mark { background: none; color: var(--text); box-shadow: inset 0 -1px 0 var(--muted); }
  .m-dim { color: var(--muted); }
  .m-field { display: grid; gap: 1px; margin-top: 6px; }
  .m-field small { font-size: 9.5px; color: var(--faint); }
  .m-field span { font-size: 11px; padding: 3px 6px; border: 1px solid var(--line); border-radius: 3px; background: var(--sunk); }
  .m-arrow { position: absolute; width: 24px; height: 1.3px; background: var(--muted); }
  .m-arrow::after { content: ""; position: absolute; right: 0; top: -3.5px; width: 7px; height: 7px; border: solid var(--muted); border-width: 1.3px 1.3px 0 0; transform: rotate(45deg); }

  /* Таблица «набираете — получаете» для заметок. */
  .md { margin-top: 14px; border: 1px solid var(--line); border-radius: var(--r-m); overflow: hidden; background: var(--sunk); }
  .md-row { display: grid; grid-template-columns: minmax(0, 1fr) 40px minmax(0, 1fr); align-items: center; padding: 11px 16px; border-bottom: 1px solid var(--line); }
  .md-row:last-child { border-bottom: 0; }
  .md-row code { font: 12.5px var(--mono); color: var(--muted); }
  .md-to { height: 1.3px; background: var(--line-strong); position: relative; margin-right: 10px; }
  .md-to::after { content: ""; position: absolute; right: 0; top: -3px; width: 6px; height: 6px; border: solid var(--line-strong); border-width: 1.3px 1.3px 0 0; transform: rotate(45deg); }
  .md-h { font: 600 18px/1.2 var(--font-display); }
  .md-li::before { content: "•"; margin-right: 8px; color: var(--muted); }
  .md-task { display: inline-flex; align-items: center; gap: 8px; }
  .md-task i { width: 13px; height: 13px; border: 1px solid var(--muted); border-radius: 3px; }
  .md-q { padding-left: 10px; border-left: 2px solid var(--line-strong); color: var(--muted); }
  .md-code { font: 12px var(--mono); padding: 3px 8px; border: 1px solid var(--line); border-radius: 3px; background: var(--bg); }

  .facts { display: grid; gap: 10px; }
  .facts p { margin: 0; padding: 12px 0; border-bottom: 1px solid var(--line); max-width: 66ch; line-height: 1.6; }

  @media (max-width: 760px) {
    .md-row { grid-template-columns: 1fr; gap: 6px; }
    .md-to { display: none; }
  }
</style>
