const invoke = window.__TAURI__.core.invoke;

const homeScreen = document.querySelector("#home-screen");
const dashboardScreen = document.querySelector("#dashboard-screen");
const form = document.querySelector("#room-form");
const examInput = document.querySelector("#exam-name");
const courseInput = document.querySelector("#course-name");
const createButton = document.querySelector("#create-button");
const roomError = document.querySelector("#room-error");
const stopButton = document.querySelector("#stop-button");
const title = document.querySelector("#dashboard-title");
const emptyState = document.querySelector("#empty-state");
const grid = document.querySelector("#student-grid");
const searchInput = document.querySelector("#student-search");
const logDir = document.querySelector("#log-dir");
const joinCodeChip = document.querySelector("#join-code-chip");
const joinCodeValue = document.querySelector("#join-code");
const joinPort = document.querySelector("#join-port");
const examOther = document.querySelector("#exam-other");
const courseOther = document.querySelector("#course-other");
const statusBadges = document.querySelector("#status-badges");
const bannerCode = document.querySelector("#banner-code");
const bannerPort = document.querySelector("#banner-port");
const bannerCopy = document.querySelector("#banner-copy");
const projectButton = document.querySelector("#project-button");
const projector = document.querySelector("#projector");
const projectorCode = document.querySelector("#projector-code");
const clearEvidence = document.querySelector("#clear-evidence");
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

// A dropdown value, or whatever was typed when "Other…" is chosen.
function fieldValue(select, other) {
  return select.value === "__other" ? other.value.trim() : select.value.trim();
}

function syncOther(select, other) {
  const isOther = select.value === "__other";
  other.hidden = !isOther;
  if (!isOther) other.value = "";
}

function isValid() {
  return (
    fieldValue(examInput, examOther).length > 0 &&
    fieldValue(courseInput, courseOther).length > 0
  );
}

function updateFormState() {
  createButton.disabled = !isValid();
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

  const heading = [status.exam_name, status.course_name].filter(Boolean).join(" : ");
  title.textContent = heading ? heading.toUpperCase() : "EXAM MONITORING";
  latestStudents = status.students;

  const connected = latestStudents.filter((s) => s.connected).length;
  const noSignal = latestStudents.filter(
    (s) => s.connected && (Date.now() - Number(s.last_update_ms)) / 1000 > STALE_SECONDS
  ).length;
  const offline = latestStudents.length - connected;

  // Problems only appear when they exist, so any badge beyond the green one
  // is itself the alert — including the two that explain an *absent* student.
  renderBadges([
    { cls: connected > 0 ? "green" : "idle", count: connected, label: "connected" },
    { cls: "amber", count: noSignal, label: "no signal" },
    { cls: "red", count: offline, label: "disconnected" },
    { cls: "purple", count: status.wrong_code_attempts, label: "wrong code" },
    { cls: "slate", count: status.unreachable_devices, label: "can't reach room" }
  ]);

  logDir.textContent = status.log_dir ? `Saving evidence to: ${status.log_dir}` : "";

  if (status.join_code) {
    const portHint = status.port ? `port ${status.port} · for students on an older client` : "";
    joinCodeValue.textContent = status.join_code;
    joinPort.textContent = status.port ? `port ${status.port} · older clients` : "";
    bannerCode.textContent = status.join_code;
    bannerPort.textContent = portHint;
    projectorCode.textContent = status.join_code;
    // One code on screen at a time: the big banner owns the empty room, the
    // toolbar chip takes over once there are tiles to watch.
    joinCodeChip.hidden = latestStudents.length === 0;
  } else {
    joinCodeChip.hidden = true;
  }

  renderStudents();
  refreshDialog();
}

function renderBadges(rows) {
  const visible = rows.filter((row) => row.count > 0 || row.cls === "idle");
  statusBadges.innerHTML = "";
  for (const row of visible) {
    const badge = document.createElement("span");
    badge.className = `badge ${row.cls}`;
    const count = document.createElement("b");
    count.textContent = row.count;
    badge.append(count, document.createTextNode(` ${row.label}`));
    statusBadges.append(badge);
  }
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
  // A generated device id identifies the machine, not the student — never show it.
  const shownId = student.student_id.startsWith("dev:") ? "" : student.student_id;
  entry.sid.textContent = shownId;
  entry.sid.hidden = !shownId;
  entry.tile.classList.toggle("blocked", student.capture_blocked === true);
  entry.statusEl.textContent = student.capture_blocked
    ? "⚠ screen blocked"
    : statusLabel(student, state);
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
  dialogId.textContent = student.capture_blocked
    ? "⚠ this student's screen recording permission is blocked"
    : student.student_id.startsWith("dev:")
      ? ""
      : student.student_id;

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

async function copyClassCode() {
  const value = joinCodeValue.textContent.trim();
  if (!value || value === "----") return;
  try {
    await navigator.clipboard?.writeText(value);
    joinCodeChip.classList.add("copied");
    window.setTimeout(() => joinCodeChip.classList.remove("copied"), 1200);
  } catch (error) {
    console.error("copy failed", error);
  }
}

function setProjector(open) {
  projector.classList.toggle("hidden", !open);
}

for (const input of [examInput, courseInput, examOther, courseOther]) {
  input.addEventListener("input", updateFormState);
}

examInput.addEventListener("change", () => {
  syncOther(examInput, examOther);
  updateFormState();
});
courseInput.addEventListener("change", () => {
  syncOther(courseInput, courseOther);
  updateFormState();
});

joinCodeChip.addEventListener("click", copyClassCode);
bannerCopy.addEventListener("click", copyClassCode);
projectButton.addEventListener("click", () => setProjector(true));
projector.addEventListener("click", () => setProjector(false));
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !projector.classList.contains("hidden")) {
    setProjector(false);
  }
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  roomError.hidden = true;

  try {
    await invoke("start_server", {
      examName: fieldValue(examInput, examOther),
      courseName: fieldValue(courseInput, courseOther)
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
  clearEvidence.checked = false;
  setProjector(false);
  setScreen("home");
});

searchInput.addEventListener("input", () => {
  searchTerm = searchInput.value;
  renderStudents();
});

clearEvidence.addEventListener("change", () => {
  invoke("set_clear_evidence", { enabled: clearEvidence.checked }).catch((error) => {
    console.error("set_clear_evidence failed", error);
  });
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
