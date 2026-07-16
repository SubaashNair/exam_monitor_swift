const invoke = window.__TAURI__.core.invoke;

const homeScreen = document.querySelector("#home-screen");
const dashboardScreen = document.querySelector("#dashboard-screen");
const form = document.querySelector("#room-form");
const examInput = document.querySelector("#exam-name");
const courseInput = document.querySelector("#course-name");
const roomInput = document.querySelector("#room-number");
const generateRoomButton = document.querySelector("#generate-room");
const copyRoomButton = document.querySelector("#copy-room");
const copyStatus = document.querySelector("#copy-status");
const createButton = document.querySelector("#create-button");
const roomError = document.querySelector("#room-error");
const stopButton = document.querySelector("#stop-button");
const title = document.querySelector("#dashboard-title");
const subtitle = document.querySelector("#dashboard-subtitle");
const emptyState = document.querySelector("#empty-state");
const grid = document.querySelector("#student-grid");
const removeColumn = document.querySelector("#remove-column");
const addColumn = document.querySelector("#add-column");
const columnLabel = document.querySelector("#column-label");
const dialog = document.querySelector("#student-dialog");
const closeDialog = document.querySelector("#close-dialog");
const dialogName = document.querySelector("#dialog-name");
const dialogId = document.querySelector("#dialog-id");
const dialogImageWrap = document.querySelector("#dialog-image-wrap");

let columns = 1;
let pollTimer = null;
let latestStudents = [];

function isValid() {
  return (
    examInput.value.trim().length > 0 &&
    courseInput.value.trim().length > 0 &&
    roomInput.value.trim().length === 4
  );
}

function updateFormState() {
  createButton.disabled = !isValid();
  copyRoomButton.disabled = roomInput.value.trim().length !== 4;
}

function setScreen(screen) {
  homeScreen.classList.toggle("hidden", screen !== "home");
  dashboardScreen.classList.toggle("hidden", screen !== "dashboard");
}

function updateColumns(next) {
  columns = Math.max(1, Math.min(6, next));
  grid.style.gridTemplateColumns = `repeat(${columns}, minmax(180px, 1fr))`;
  columnLabel.textContent = `${columns} column${columns === 1 ? "" : "s"}`;
  removeColumn.disabled = columns <= 1;
  addColumn.disabled = columns >= 6;
}

async function pollStatus() {
  const status = await invoke("server_status");
  title.textContent = status.exam_name || "Exam Monitoring";

  const parts = [];
  if (status.course_name) parts.push(status.course_name);
  if (status.room_number) parts.push(`Class ${status.room_number}`);
  parts.push(`${status.students.length} connected`);
  subtitle.textContent = parts.join(" - ");

  latestStudents = status.students;
  renderStudents();
}

function renderStudents() {
  emptyState.classList.toggle("hidden", latestStudents.length > 0);
  grid.classList.toggle("hidden", latestStudents.length === 0);

  grid.innerHTML = "";
  for (const student of latestStudents) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "student-tile";
    item.addEventListener("click", () => openStudent(student));

    const preview = document.createElement("div");
    preview.className = "preview";

    if (student.image_data_url) {
      const image = document.createElement("img");
      image.src = student.image_data_url;
      image.alt = "";
      preview.append(image);
    } else {
      preview.textContent = "Display";
    }

    const footer = document.createElement("div");
    footer.className = "student-footer";
    footer.innerHTML = `
      <span>
        <strong>${escapeHtml(student.name)}</strong>
        ${student.student_id ? `<small>${escapeHtml(student.student_id)}</small>` : ""}
      </span>
      <small>${statusLabel(student)}</small>
    `;

    item.append(preview, footer);
    grid.append(item);
  }
}

function openStudent(student) {
  dialogName.textContent = student.name;
  dialogId.textContent = student.student_id;
  dialogImageWrap.innerHTML = "";

  if (student.image_data_url) {
    const image = document.createElement("img");
    image.src = student.image_data_url;
    image.alt = "";
    dialogImageWrap.append(image);
  } else {
    dialogImageWrap.textContent = "Display";
  }

  dialog.showModal();
}

function statusLabel(student) {
  const sinceFrame = Math.floor((Date.now() - Number(student.last_update_ms)) / 1000);
  if (sinceFrame > 15) {
    return `⚠ no signal ${sinceFrame}s`;
  }
  return `connected ${formatDuration(Number(student.connected_at_ms))}`;
}

function formatDuration(startMs) {
  const seconds = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
  return `${seconds}s`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function startPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
  }
  pollStatus();
  pollTimer = window.setInterval(pollStatus, 1000);
}

function stopPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
}

function generateClassNumber() {
  return String(Math.floor(Math.random() * 8000) + 2000);
}

async function copyClassNumber() {
  const value = roomInput.value.trim();
  if (value.length !== 4) return;

  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
    } else {
      roomInput.select();
      document.execCommand("copy");
      roomInput.blur();
    }

    copyStatus.textContent = "Copied";
  } catch (error) {
    copyStatus.textContent = "Copy failed";
  }
}

for (const input of [examInput, courseInput, roomInput]) {
  input.addEventListener("input", updateFormState);
}

roomInput.addEventListener("input", () => {
  roomInput.value = roomInput.value.replace(/\D/g, "").slice(0, 4);
  copyStatus.textContent = "Share this class number with students";
  updateFormState();
});

generateRoomButton.addEventListener("click", () => {
  roomInput.value = generateClassNumber();
  roomError.hidden = true;
  copyStatus.textContent = "Share this class number with students";
  updateFormState();
  roomInput.focus();
});

copyRoomButton.addEventListener("click", copyClassNumber);

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  roomError.hidden = true;

  try {
    await invoke("start_server", {
      examName: examInput.value.trim(),
      courseName: courseInput.value.trim(),
      roomNumber: roomInput.value.trim()
    });

    setScreen("dashboard");
    updateColumns(columns);
    startPolling();
  } catch (error) {
    roomError.textContent = String(error);
    roomError.hidden = false;
  }
});

stopButton.addEventListener("click", async () => {
  await invoke("stop_server");
  stopPolling();
  latestStudents = [];
  setScreen("home");
});

removeColumn.addEventListener("click", () => updateColumns(columns - 1));
addColumn.addEventListener("click", () => updateColumns(columns + 1));
closeDialog.addEventListener("click", () => dialog.close());

updateColumns(columns);
updateFormState();

invoke("app_version").then((version) => {
  document.querySelector("#app-version").textContent = `v${version}`;
});
