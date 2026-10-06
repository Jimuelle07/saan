// Minimal TODO list app in plain TypeScript - no framework, just the DOM.
// Items persist in localStorage.

interface Todo {
  id: number;
  title: string;
  done: boolean;
}

const STORAGE_KEY = "todos.v1";

function load(): Todo[] {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]") as Todo[];
  } catch {
    return [];
  }
}

function save(todos: Todo[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(todos));
}

let todos = load();

function render(list: HTMLUListElement, counter: HTMLElement): void {
  list.replaceChildren(
    ...todos.map((todo) => {
      const li = document.createElement("li");
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = todo.done;
      box.addEventListener("change", () => toggle(todo.id));
      const label = document.createElement("span");
      label.textContent = todo.title;
      label.className = todo.done ? "done" : "";
      const remove = document.createElement("button");
      remove.textContent = "x";
      remove.addEventListener("click", () => removeTodo(todo.id));
      li.append(box, label, remove);
      return li;
    }),
  );
  const left = todos.filter((t) => !t.done).length;
  counter.textContent = `${left} item${left === 1 ? "" : "s"} left`;
}

function update(next: Todo[]): void {
  todos = next;
  save(todos);
  render(
    document.querySelector<HTMLUListElement>("#todo-list")!,
    document.querySelector<HTMLElement>("#counter")!,
  );
}

function addTodo(title: string): void {
  if (!title.trim()) return;
  update([...todos, { id: Date.now(), title: title.trim(), done: false }]);
}

function toggle(id: number): void {
  update(todos.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));
}

function removeTodo(id: number): void {
  update(todos.filter((t) => t.id !== id));
}

document.querySelector<HTMLFormElement>("#new-todo")!.addEventListener("submit", (e) => {
  e.preventDefault();
  const input = (e.currentTarget as HTMLFormElement).elements.namedItem("title") as HTMLInputElement;
  addTodo(input.value);
  input.value = "";
});

update(todos);
