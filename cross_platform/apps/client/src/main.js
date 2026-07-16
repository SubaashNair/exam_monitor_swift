const invoke = window.__TAURI__.core.invoke;

const joinScreen = document.querySelector("#join-screen");
const dashboardScreen = document.querySelector("#dashboard-screen");
const form = document.querySelector("#join-form");
const nameInput = document.querySelector("#student-name");
const idInput = document.querySelector("#student-id");
const roomInput = document.querySelector("#room-number");
const joinCodeInput = document.querySelector("#join-code");
const serverIpInput = document.querySelector("#server-ip");
const startButton = document.querySelector("#start-button");
const stopButton = document.querySelector("#stop-button");
const joinError = document.querySelector("#join-error");
const statusMessage = document.querySelector("#status-message");
const statusDot = document.querySelector("#status-dot");
const connectionLabel = document.querySelector("#connection-label");
const summaryName = document.querySelector("#summary-name");
const summaryId = document.querySelector("#summary-id");
const summaryRoom = document.querySelector("#summary-room");

let pollTimer = null;

function isValid() {
  return (
    nameInput.value.trim().length > 0 &&
    idInput.value.trim().length >= 3 &&
    Number.parseInt(roomInput.value, 10) > 0 &&
    joinCodeInput.value.trim().length === 4
  );
}

function updateFormState() {
  startButton.disabled = !isValid();
}

function setScreen(screen) {
  joinScreen.classList.toggle("hidden", screen !== "join");
  dashboardScreen.classList.toggle("hidden", screen !== "dashboard");
}

async function pollStatus() {
  const status = await invoke("client_status");
  statusMessage.textContent = status.status;
  connectionLabel.textContent = status.is_connected ? "Connected" : "Not connected";
  statusDot.classList.toggle("connected", status.is_connected);
  statusDot.classList.toggle("waiting", !status.is_connected);
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

for (const input of [nameInput, idInput, roomInput, joinCodeInput]) {
  input.addEventListener("input", updateFormState);
}

roomInput.addEventListener("input", () => {
  roomInput.value = roomInput.value.replace(/\D/g, "").slice(0, 4);
  updateFormState();
});

joinCodeInput.addEventListener("input", () => {
  // Codes use an unambiguous uppercase alphabet; normalise as they type.
  joinCodeInput.value = joinCodeInput.value.toUpperCase().replace(/[^A-Z2-9]/g, "").slice(0, 4);
  updateFormState();
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  joinError.hidden = true;

  try {
    await invoke("start_client", {
      studentName: nameInput.value.trim(),
      studentId: idInput.value.trim(),
      roomNumber: roomInput.value.trim(),
      serverIp: serverIpInput.value.trim() || null,
      joinCode: joinCodeInput.value.trim()
    });

    summaryName.textContent = nameInput.value.trim();
    summaryId.textContent = idInput.value.trim();
    summaryRoom.textContent = roomInput.value.trim();
    setScreen("dashboard");
    startPolling();
  } catch (error) {
    joinError.textContent = String(error);
    joinError.hidden = false;
  }
});

stopButton.addEventListener("click", async () => {
  await invoke("stop_client");
  stopPolling();
  setScreen("join");
});

updateFormState();


invoke("app_version").then((version) => {
  document.querySelector("#app-version").textContent = `v${version}`;
});
