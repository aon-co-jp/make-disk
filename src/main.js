// バンドラー(Vite等)を使わない素のWebView(特にAndroidのSystem WebView)は
// "@tauri-apps/api/core"のようなベア指定子を解決できず、モジュール読み込みが
// 例外で止まり、main.js内の全イベントリスナーが登録されないまま失敗する
// (実機検証で発見: ボタンが一切反応しない不具合の原因だった)。
// tauri.conf.jsonのwithGlobalTauri:trueで公開されるグローバルを使うことで、
// バンドラー無しでも全プラットフォームで確実に動く。
const invoke = window.__TAURI__.core.invoke;
const open = window.__TAURI__.dialog.open;
const convertFileSrc = window.__TAURI__.core.convertFileSrc;

/**
 * @typedef {{ startSecs: number, endSecs: number | null }} CutRange
 * @typedef {{ path: string, cutRanges: CutRange[], frameAccurate: boolean, editing: boolean }} SourceFile
 */
/** @type {SourceFile[]} */
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

function secsToHms(totalSecs) {
  const s = Math.max(0, Math.floor(totalSecs));
  return { h: Math.floor(s / 3600), m: Math.floor((s % 3600) / 60), sec: s % 60 };
}

function hmsToSecs(h, m, sec) {
  return (parseInt(h, 10) || 0) * 3600 + (parseInt(m, 10) || 0) * 60 + (parseFloat(sec) || 0);
}

function formatHms(totalSecs) {
  const { h, m, sec } = secsToHms(totalSecs);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

/** 開始/終了を時・分・秒の数値入力3つ(+任意の"末尾まで"チェック)として
 * 描画するミニフォーム。マウス操作(動画プレビューの現在位置をセット)と
 * 数字直接入力のどちらでも同じ値を編集できる。 */
function buildTimeInputs(labelText, initialSecs, videoEl, onSetFromPlayhead) {
  const wrap = document.createElement("span");
  wrap.className = "hms-input";

  const { h, m, sec } = secsToHms(initialSecs ?? 0);
  const hEl = document.createElement("input");
  hEl.type = "number";
  hEl.min = "0";
  hEl.value = h;
  hEl.className = "hms-h";
  const mEl = document.createElement("input");
  mEl.type = "number";
  mEl.min = "0";
  mEl.max = "59";
  mEl.value = m;
  mEl.className = "hms-m";
  const sEl = document.createElement("input");
  sEl.type = "number";
  sEl.min = "0";
  sEl.max = "59";
  sEl.value = sec;
  sEl.className = "hms-s";

  const setBtn = document.createElement("button");
  setBtn.type = "button";
  setBtn.textContent = "現在位置";
  setBtn.title = "動画プレビューの再生位置をこの欄にセットします";
  setBtn.addEventListener("click", () => {
    const { h: ch, m: cm, sec: cs } = secsToHms(videoEl.currentTime);
    hEl.value = ch;
    mEl.value = cm;
    sEl.value = cs;
    if (onSetFromPlayhead) onSetFromPlayhead();
  });

  const label = document.createElement("span");
  label.textContent = labelText;

  wrap.append(label, hEl, document.createTextNode(":"), mEl, document.createTextNode(":"), sEl, setBtn);
  return { el: wrap, getSecs: () => hmsToSecs(hEl.value, mEl.value, sEl.value) };
}

function renderCutEditor(container, file, index) {
  container.innerHTML = "";
  container.className = "cut-editor";

  const isVideo = /\.(mp4|mkv|avi|mov|webm)$/i.test(file.path);
  let video = null;
  if (isVideo) {
    video = document.createElement("video");
    video.controls = true;
    video.preload = "metadata";
    video.style.maxWidth = "100%";
    video.src = convertFileSrc(file.path);
    container.appendChild(video);
  } else {
    const note = document.createElement("p");
    note.className = "hint";
    note.textContent = "音声ファイルのプレビューは未対応です。数字で時:分:秒を直接指定してください。 / Preview isn't available for audio files — enter times directly.";
    container.appendChild(note);
  }

  const rangeList = document.createElement("ul");
  rangeList.className = "cut-range-list";
  function renderRangeList() {
    rangeList.innerHTML = "";
    file.cutRanges.forEach((r, ri) => {
      const li = document.createElement("li");
      li.textContent = `カット ${ri + 1}: ${formatHms(r.startSecs)} 〜 ${r.endSecs === null ? "末尾まで" : formatHms(r.endSecs)}`;
      const removeBtn = document.createElement("button");
      removeBtn.textContent = "削除";
      removeBtn.addEventListener("click", () => {
        file.cutRanges.splice(ri, 1);
        renderRangeList();
      });
      li.appendChild(removeBtn);
      rangeList.appendChild(li);
    });
  }
  renderRangeList();

  const addForm = document.createElement("div");
  addForm.className = "cut-range-form";

  const dummyVideo = video ?? { currentTime: 0 };
  const startInput = buildTimeInputs("開始", 0, dummyVideo);
  const endInput = buildTimeInputs("終了", 0, dummyVideo);

  const toEofCheckbox = document.createElement("input");
  toEofCheckbox.type = "checkbox";
  const toEofLabel = document.createElement("label");
  toEofLabel.append(toEofCheckbox, document.createTextNode(" 末尾までカット"));

  const addBtn = document.createElement("button");
  addBtn.textContent = "この区間をカットに追加";
  addBtn.addEventListener("click", () => {
    const startSecs = startInput.getSecs();
    const endSecs = toEofCheckbox.checked ? null : endInput.getSecs();
    if (endSecs !== null && endSecs <= startSecs) {
      log("エラー: 終了位置は開始位置より後にしてください。 / End must be after start.");
      return;
    }
    file.cutRanges.push({ startSecs, endSecs });
    renderRangeList();
  });

  addForm.append(startInput.el, endInput.el, toEofLabel, addBtn);

  const frameAccurateLabel = document.createElement("label");
  const frameAccurateCheckbox = document.createElement("input");
  frameAccurateCheckbox.type = "checkbox";
  frameAccurateCheckbox.checked = file.frameAccurate;
  frameAccurateCheckbox.addEventListener("change", () => (file.frameAccurate = frameAccurateCheckbox.checked));
  frameAccurateLabel.append(
    frameAccurateCheckbox,
    document.createTextNode(" フレーム精度で正確にカットする(GPUエンコーダがあれば自動使用、無ければCPU) / Frame-accurate cut (uses GPU encoder if available)")
  );

  container.append(document.createElement("h4"), rangeList, addForm, frameAccurateLabel);
  container.querySelector("h4").textContent = "カットする区間(いくつでも追加可)";

  if (video) {
    video.addEventListener("loadedmetadata", async () => {
      try {
        const estimate = await invoke("estimate_cpu_encode_speed");
        if (estimate.speed_hint === "slow") {
          log(`ℹ️ ${estimate.message_ja} / ${estimate.message_en}`);
        }
      } catch (e) {
        // 参考情報の取得失敗は無視してよい(必須機能ではない)。
      }
    });
  }
}

function renderFileList() {
  fileListEl.innerHTML = "";
  sourceFiles.forEach((f, i) => {
    const li = document.createElement("li");
    li.className = "file-row";

    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = f.path + (f.cutRanges.length > 0 ? ` (カット${f.cutRanges.length}件)` : "");

    const editBtn = document.createElement("button");
    editBtn.textContent = f.editing ? "閉じる" : "編集...";
    editBtn.addEventListener("click", () => {
      f.editing = !f.editing;
      renderFileList();
    });

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "削除";
    removeBtn.addEventListener("click", () => {
      sourceFiles.splice(i, 1);
      renderFileList();
    });

    li.append(name, editBtn, removeBtn);
    fileListEl.appendChild(li);

    if (f.editing) {
      const editorLi = document.createElement("li");
      renderCutEditor(editorLi, f, i);
      fileListEl.appendChild(editorLi);
    }
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
    sourceFiles.push({ path, cutRanges: [], frameAccurate: false, editing: false });
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

/** カット区間を考慮した実効尺(自動ビットレート算出用)。
 * カット区間の合計を元の尺から差し引く。 */
async function effectiveDurationSecs(f) {
  let total = 0;
  try {
    const info = await invoke("probe_media", { path: f.path });
    total = info.duration_secs;
  } catch (e) {
    log(`警告: ${f.path} の尺取得に失敗: ${e}`);
    return 0;
  }
  for (const r of f.cutRanges) {
    const end = r.endSecs ?? total;
    total -= Math.max(0, end - r.startSecs);
  }
  return Math.max(0, total);
}

/** 選択された複数ディスク種別のうち、最も容量が小さいものを基準に
 * 自動ビットレートを算出する(=どのディスクにも収まる保守的な値)。 */
async function computeAutoBitrateKbps(discTypes) {
  let totalDuration = 0;
  for (const f of sourceFiles) {
    totalDuration += await effectiveDurationSecs(f);
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
            trim: null,
            cut_ranges: f.cutRanges.length > 0 ? f.cutRanges.map((r) => ({ start_secs: r.startSecs, end_secs: r.endSecs })) : null,
            frame_accurate: f.frameAccurate,
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
