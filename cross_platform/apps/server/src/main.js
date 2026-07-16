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
const searchInput = document.querySelector("#student-search");
const logDir = document.querySelector("#log-dir");
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

const STALE_SECONDS = 15;
const tileEls = new Map();
let openStudentId = null;
let searchTerm = "";

function studentState(student) {
  if (!student.connected) return "offline";
  const sinceFrame = (Date.now() - Number(student.last_update_ms)) / 1000;
  return sinceFrame > STALE_SECONDS ? "stale" : "ok";
}

function statusLabel(student, state) {
  if (state === "offline") {
    const since = Number(student.disconnected_at_ms ?? student.last_update_ms);
    return `disconnected ${formatDuration(since)} ago`;
  }
  if (state === "stale") {
    const seconds = Math.floor((Date.now() - Number(student.last_update_ms)) / 1000);
    return `⚠ no signal ${seconds}s`;
  }
  return `connected ${formatDuration(Number(student.connected_at_ms))}`;
}

// Problems first (offline, then stale), then alphabetical, so the proctor's
// attention lands on the tiles that need it.
function visibleStudents() {
  const term = searchTerm.trim().toLowerCase();
  const rank = { offline: 0, stale: 1, ok: 2 };
  return latestStudents
    .filter((student) => {
      if (!term) return true;
      return (
        student.name.toLowerCase().includes(term) ||
        student.student_id.toLowerCase().includes(term)
      );
    })
    .map((student) => ({ student, state: studentState(student) }))
    .sort((a, b) => {
      const byRank = rank[a.state] - rank[b.state];
      return byRank !== 0 ? byRank : a.student.name.localeCompare(b.student.name);
    });
}

async function pollStatus() {
  let status;
  try {
    status = await invoke("server_status");
  } catch (error) {
    console.error("server_status failed", error);
    return;
  }

  title.textContent = status.exam_name || "Exam Monitoring";
  latestStudents = status.students;

  const connected = latestStudents.filter((s) => s.connected).length;
  const noSignal = latestStudents.filter(
    (s) => s.connected && (Date.now() - Number(s.last_update_ms)) / 1000 > STALE_SECONDS
  ).length;
  const offline = latestStudents.length - connected;

  const parts = [];
  if (status.course_name) parts.push(status.course_name);
  if (status.room_number) parts.push(`Class ${status.room_number}`);
  parts.push(`${connected} connected`);
  if (noSignal > 0) parts.push(`${noSignal} no-signal`);
  if (offline > 0) parts.push(`${offline} disconnected`);
  subtitle.textContent = parts.join(" · ");

  logDir.textContent = status.log_dir ? `Saving evidence to: ${status.log_dir}` : "";

  renderStudents();
  refreshDialog();
}

// Keyed reconciliation: reuse existing tiles and only touch what changed, so
// images don't flicker or re-decode every second and tiles keep their place.
function renderStudents() {
  const rows = visibleStudents();
  emptyState.classList.toggle("hidden", rows.length > 0);
  grid.classList.toggle("hidden", rows.length === 0);

  const seen = new Set();
  rows.forEach(({ student, state }, index) => {
    seen.add(student.id);
    let entry = tileEls.get(student.id);
    if (!entry) {
      entry = createTile(student.id);
      tileEls.set(student.id, entry);
    }
    updateTile(entry, student, state);
    if (grid.children[index] !== entry.tile) {
      grid.insertBefore(entry.tile, grid.children[index] || null);
    }
  });

  for (const [id, entry] of tileEls) {
    if (!seen.has(id)) {
      entry.tile.remove();
      tileEls.delete(id);
    }
  }
}

function createTile(id) {
  const tile = document.createElement("div");
  tile.className = "student-tile";
  tile.tabIndex = 0;
  tile.addEventListener("click", () => openStudent(id));

  const preview = document.createElement("div");
  preview.className = "preview";
  const img = document.createElement("img");
  img.alt = "";
  img.hidden = true;
  const placeholder = document.createElement("span");
  placeholder.className = "preview-placeholder";
  placeholder.textContent = "Display";
  preview.append(img, placeholder);

  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.className = "dismiss";
  dismiss.textContent = "×";
  dismiss.title = "Dismiss";
  dismiss.hidden = true;
  dismiss.addEventListener("click", (event) => {
    event.stopPropagation();
    invoke("dismiss_student", { studentId: id }).catch(() => {});
  });

  const footer = document.createElement("div");
  footer.className = "student-footer";
  const nameWrap = document.createElement("span");
  const name = document.createElement("strong");
  const sid = document.createElement("small");
  nameWrap.append(name, sid);
  const statusEl = document.createElement("small");
  footer.append(nameWrap, statusEl);

  tile.append(preview, dismiss, footer);
  return { tile, img, placeholder, dismiss, name, sid, statusEl, src: null };
}

function updateTile(entry, student, state) {
  entry.tile.classList.toggle("stale", state === "stale");
  entry.tile.classList.toggle("offline", state === "offline");

  const src = student.image_data_url || null;
  if (src !== entry.src) {
    entry.src = src;
    if (src) {
      entry.img.src = src;
      entry.img.hidden = false;
      entry.placeholder.hidden = true;
    } else {
      entry.img.removeAttribute("src");
      entry.img.hidden = true;
      entry.placeholder.hidden = false;
    }
  }

  entry.name.textContent = student.name;
  entry.sid.textContent = student.student_id;
  entry.sid.hidden = !student.student_id;
  entry.statusEl.textContent = statusLabel(student, state);
  entry.dismiss.hidden = state !== "offline";
}

function openStudent(id) {
  openStudentId = id;
  refreshDialog();
  if (!dialog.open) dialog.showModal();
}

// Keep the open zoom view live instead of frozen at click time.
function refreshDialog() {
  if (openStudentId === null) return;
  const student = latestStudents.find((s) => s.id === openStudentId);
  if (!student) {
    dialog.close();
    return;
  }

  dialogName.textContent = student.name;
  dialogId.textContent = student.student_id;

  const src = student.image_data_url || null;
  if (src) {
    let img = dialogImageWrap.querySelector("img");
    if (!img) {
      dialogImageWrap.textContent = "";
      img = document.createElement("img");
      img.alt = "";
      dialogImageWrap.append(img);
    }
    if (img.getAttribute("src") !== src) img.src = src;
  } else {
    dialogImageWrap.textContent = "Display";
  }
}

function formatDuration(startMs) {
  const seconds = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
  return `${seconds}s`;
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
  for (const entry of tileEls.values()) entry.tile.remove();
  tileEls.clear();
  if (dialog.open) dialog.close();
  setScreen("home");
});

searchInput.addEventListener("input", () => {
  searchTerm = searchInput.value;
  renderStudents();
});

removeColumn.addEventListener("click", () => updateColumns(columns - 1));
addColumn.addEventListener("click", () => updateColumns(columns + 1));
closeDialog.addEventListener("click", () => dialog.close());
dialog.addEventListener("close", () => {
  openStudentId = null;
});

updateColumns(columns);
updateFormState();

invoke("app_version").then((version) => {
  document.querySelector("#app-version").textContent = `v${version}`;
});
