// バンドラー(Vite等)を使わない素のWebView(特にAndroidのSystem WebView)は
// "@tauri-apps/api/core"のようなベア指定子を解決できず、モジュール読み込みが
// 例外で止まり、main.js内の全イベントリスナーが登録されないまま失敗する
// (実機検証で発見: ボタンが一切反応しない不具合の原因だった)。
// tauri.conf.jsonのwithGlobalTauri:trueで公開されるグローバルを使うことで、
// バンドラー無しでも全プラットフォームで確実に動く。
const invoke = window.__TAURI__.core.invoke;
const open = window.__TAURI__.dialog.open;

/** @type {{path: string, startSecs: string, durationSecs: string}[]} */
let sourceFiles = [];
let outputFolder = "";

const fileListEl = document.getElementById("file-list");
const outputFolderEl = document.getElementById("output-folder");
const logEl = document.getElementById("log");

const VIDEO_CODEC_ARGS = {
  mp4: ["-c:v", "libx264", "-c:a", "aac"],
  mkv: ["-c:v", "libx264", "-c:a", "aac"],
  avi: ["-c:v", "mpeg4", "-c:a", "libmp3lame"],
  mov: ["-c:v", "libx264", "-c:a", "aac"],
  webm: ["-c:v", "libvpx-vp9", "-c:a", "libopus"],
};

const AUDIO_CODEC_ARGS = {
  mp3: ["-c:a", "libmp3lame"],
  wav: ["-c:a", "pcm_s16le"],
  flac: ["-c:a", "flac"],
  aac: ["-c:a", "aac"],
  ogg: ["-c:a", "libvorbis"],
};

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
  try {
    const dir = await open({ directory: true });
    if (!dir) return;
    outputFolder = dir;
    outputFolderEl.value = dir;
  } catch (e) {
    if (String(e).includes("not implemented on mobile")) {
      // TODO(次回セッション): SAF(ACTION_OPEN_DOCUMENT_TREE)を扱う独自
      // Tauriプラグインを実装し、モバイルでも共通フォルダへの出力を
      // 可能にする計画。詳細はCLAUDE.md「Android実機検証で発見した
      // 問題と対応方針」の発見2を参照。現時点では未実装のため案内のみ。
      log(
        "このプラットフォームでは共通フォルダの選択に未対応です(実装準備中)。 / " +
        "Shared folder selection is not yet supported on this platform (implementation planned)."
      );
    } else {
      log(`エラー: ${e}`);
    }
  }
});

document.getElementById("write-speed-mode").addEventListener("change", (e) => {
  const fixedInput = document.getElementById("write-speed-fixed");
  fixedInput.style.display = e.target.value === "fixed" ? "inline-block" : "none";
});

function log(msg) {
  logEl.textContent += msg + "\n";
}

function checkedValues(name) {
  return Array.from(document.querySelectorAll(`input[name="${name}"]:checked`)).map((el) => el.value);
}

function bitrateMode() {
  return document.querySelector('input[name="bitrate-mode"]:checked').value;
}

function baseName(path) {
  return path.replace(/\\/g, "/").split("/").pop().replace(/\.[^.]+$/, "");
}

/** 選択された複数ディスク種別のうち、最も容量が小さいものを基準に
 * 自動ビットレートを算出する(=どのディスクにも収まる保守的な値)。 */
async function computeAutoBitrateKbps(discTypes) {
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

  let minKbps = null;
  for (const discType of discTypes) {
    const kbps = await invoke("calc_auto_bitrate_kbps", {
      disc: discType,
      totalDurationSecs: totalDuration,
      reservedBytes: 50 * 1024 * 1024,
    });
    if (minKbps === null || kbps < minKbps) minKbps = kbps;
  }
  return minKbps ?? 0;
}

async function convertAll(formats, codecMap, mode, bitrateKbps) {
  const outputs = [];
  for (const format of formats) {
    const codecArgs = codecMap[format];
    for (const f of sourceFiles) {
      const outputPath = `${outputFolder}/${baseName(f.path)}.${format}`;
      log(`変換中: ${f.path} -> ${outputPath}`);
      try {
        await invoke("convert_media", {
          job: {
            input_path: f.path,
            output_path: outputPath,
            codec_args: codecArgs,
            bitrate: format === "wav" || format === "flac" ? null : { [mode === "auto" ? "auto_max_for_capacity" : "fixed"]: bitrateKbps },
            trim: {
              start_secs: f.startSecs ? parseFloat(f.startSecs) : null,
              duration_secs: f.durationSecs ? parseFloat(f.durationSecs) : null,
            },
          },
        });
        outputs.push(outputPath);
        log(`完了: ${outputPath}`);
      } catch (e) {
        log(`エラー: ${e}`);
      }
    }
  }
  return outputs;
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

  const audioFormats = checkedValues("audio-format");
  const videoFormats = checkedValues("video-format");
  const discTypes = checkedValues("disc-type");
  const wantIso = document.getElementById("output-iso").checked;

  if (audioFormats.length === 0 && videoFormats.length === 0) {
    log("エラー: 音声または動画フォーマットを1つ以上選択してください。");
    return;
  }

  const mode = bitrateMode();
  let bitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10);

  if (mode === "auto") {
    if (discTypes.length === 0) {
      log("エラー: 自動ビットレートにはディスク種別を1つ以上選択してください。");
      return;
    }
    log("選択したディスクのうち最小容量に合わせて最大ビットレートを算出中...");
    bitrateKbps = await computeAutoBitrateKbps(discTypes);
    log(`自動算出ビットレート: ${bitrateKbps} kbps`);

    const mediaKind = videoFormats.length > 0 ? "video" : "audio";
    const warning = await invoke("check_bitrate_quality", { bitrateKbps, kind: mediaKind });
    if (warning) {
      log(`⚠️ ${warning.message_ja} / ${warning.message_en}`);
    }
  }

  const convertedPaths = [];
  if (audioFormats.length > 0) {
    convertedPaths.push(...(await convertAll(audioFormats, AUDIO_CODEC_ARGS, mode, bitrateKbps)));
  }
  if (videoFormats.length > 0) {
    convertedPaths.push(...(await convertAll(videoFormats, VIDEO_CODEC_ARGS, mode, bitrateKbps)));
  }

  if (wantIso || discTypes.length > 0) {
    const isoPath = `${outputFolder}/output.iso`;
    log(`ISO作成中: ${isoPath}`);
    try {
      await invoke("create_iso", {
        sourceDir: outputFolder,
        outputIso: isoPath,
        volumeLabel: "MAKE_DISK",
      });
      log(`完了: ${isoPath}`);

      if (discTypes.length > 0) {
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
          for (const discType of discTypes) {
            log(`書き込み中(${discType}): ${isoPath} -> ${device}`);
            try {
              await invoke("burn_image", { imagePath: isoPath, device, disc: discType, speed });
              log(`書き込み完了(${discType})。`);
            } catch (e) {
              log(`エラー(${discType}): ${e}`);
            }
          }
        }
      }
    } catch (e) {
      log(`エラー: ${e}`);
    }
  }

  log("すべての処理が完了しました。");
});
