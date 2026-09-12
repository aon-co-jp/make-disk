import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** @type {{path: string, startSecs: string, durationSecs: string}[]} */
let sourceFiles = [];
let outputFolder = "";

const fileListEl = document.getElementById("file-list");
const outputFolderEl = document.getElementById("output-folder");
const logEl = document.getElementById("log");

function renderFileList() {
  fileListEl.innerHTML = "";
  sourceFiles.forEach((f, i) => {
    const li = document.createElement("li");
    li.className = "file-row";

    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = f.path;

    const start = document.createElement("input");
    start.type = "number";
    start.min = "0";
    start.placeholder = "開始(秒)";
    start.value = f.startSecs;
    start.addEventListener("input", () => (sourceFiles[i].startSecs = start.value));

    const dur = document.createElement("input");
    dur.type = "number";
    dur.min = "0";
    dur.placeholder = "長さ(秒) 例:300=5分";
    dur.value = f.durationSecs;
    dur.addEventListener("input", () => (sourceFiles[i].durationSecs = dur.value));

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "削除";
    removeBtn.addEventListener("click", () => {
      sourceFiles.splice(i, 1);
      renderFileList();
    });

    li.append(name, start, dur, removeBtn);
    fileListEl.appendChild(li);
  });
}

document.getElementById("add-files-btn").addEventListener("click", async () => {
  const selected = await open({
    multiple: true,
    filters: [
      { name: "音声/動画", extensions: ["mp3", "wav", "flac", "aac", "m4a", "ogg", "mp4", "mkv", "avi", "mov", "webm", "iso"] },
      { name: "すべてのファイル", extensions: ["*"] },
    ],
  });
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  for (const path of paths) {
    sourceFiles.push({ path, startSecs: "", durationSecs: "" });
  }
  renderFileList();
});

document.getElementById("pick-output-btn").addEventListener("click", async () => {
  const dir = await open({ directory: true });
  if (!dir) return;
  outputFolder = dir;
  outputFolderEl.value = dir;
});

document.getElementById("write-speed-mode").addEventListener("change", (e) => {
  const fixedInput = document.getElementById("write-speed-fixed");
  fixedInput.style.display = e.target.value === "fixed" ? "inline-block" : "none";
});

function log(msg) {
  logEl.textContent += msg + "\n";
}

function selectedTargets() {
  return Array.from(document.querySelectorAll('input[name="target"]:checked')).map((el) => el.value);
}

function bitrateMode() {
  return document.querySelector('input[name="bitrate-mode"]:checked').value;
}

async function computeAutoBitrateKbps(discType) {
  let totalDuration = 0;
  for (const f of sourceFiles) {
    if (f.durationSecs) {
      totalDuration += parseFloat(f.durationSecs);
    } else {
      try {
        const info = await invoke("probe_media", { path: f.path });
        totalDuration += info.duration_secs;
      } catch (e) {
        log(`警告: ${f.path} の尺取得に失敗: ${e}`);
      }
    }
  }
  return invoke("calc_auto_bitrate_kbps", {
    disc: discType,
    totalDurationSecs: totalDuration,
    reservedBytes: 50 * 1024 * 1024,
  });
}

document.getElementById("run-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  if (sourceFiles.length === 0) {
    log("エラー: ソースファイルを追加してください。");
    return;
  }
  if (!outputFolder) {
    log("エラー: 出力先フォルダを選択してください。");
    return;
  }

  const targets = selectedTargets();
  if (targets.length === 0) {
    log("エラー: 出力形式を1つ以上選択してください。");
    return;
  }

  const discType = document.getElementById("disc-type").value;
  const mode = bitrateMode();
  let bitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10);

  if (mode === "auto") {
    log("ディスク容量から最大ビットレートを算出中...");
    bitrateKbps = await computeAutoBitrateKbps(discType);
    log(`自動算出ビットレート: ${bitrateKbps} kbps`);
  }

  const convertedPaths = [];

  if (targets.includes("convert_audio") || targets.includes("convert_video")) {
    for (const f of sourceFiles) {
      const isAudio = targets.includes("convert_audio");
      const ext = isAudio ? "mp3" : "mp4";
      const base = f.path.replace(/\\/g, "/").split("/").pop().replace(/\.[^.]+$/, "");
      const outputPath = `${outputFolder}/${base}.${ext}`;
      const codecArgs = isAudio ? ["-c:a", "libmp3lame"] : ["-c:v", "libx264", "-c:a", "aac"];

      log(`変換中: ${f.path} -> ${outputPath}`);
      try {
        await invoke("convert_media", {
          job: {
            input_path: f.path,
            output_path: outputPath,
            codec_args: codecArgs,
            bitrate: { [mode === "auto" ? "auto_max_for_capacity" : "fixed"]: bitrateKbps },
            trim: {
              start_secs: f.startSecs ? parseFloat(f.startSecs) : null,
              duration_secs: f.durationSecs ? parseFloat(f.durationSecs) : null,
            },
          },
        });
        convertedPaths.push(outputPath);
        log(`完了: ${outputPath}`);
      } catch (e) {
        log(`エラー: ${e}`);
      }
    }
  }

  if (targets.includes("iso")) {
    const isoPath = `${outputFolder}/output.iso`;
    log(`ISO作成中: ${isoPath}`);
    try {
      await invoke("create_iso", {
        sourceDir: outputFolder,
        outputIso: isoPath,
        volumeLabel: "MAKE_DISK",
      });
      log(`完了: ${isoPath}`);
    } catch (e) {
      log(`エラー: ${e}`);
    }
  }

  if (targets.includes("burn")) {
    try {
      const devices = await invoke("list_burn_devices");
      if (devices.length === 0) {
        log("エラー: 書き込み可能な光学ドライブが見つかりません。");
      } else {
        const device = devices[0];
        const speedMode = document.getElementById("write-speed-mode").value;
        const speed =
          speedMode === "fixed"
            ? { fixed: parseInt(document.getElementById("write-speed-fixed").value, 10) }
            : speedMode === "max"
              ? "max"
              : "auto";
        const imagePath = `${outputFolder}/output.iso`;
        log(`書き込み中: ${imagePath} -> ${device}`);
        await invoke("burn_image", { imagePath, device, disc: discType, speed });
        log("書き込み完了。");
      }
    } catch (e) {
      log(`エラー: ${e}`);
    }
  }

  log("すべての処理が完了しました。");
});
