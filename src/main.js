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
  // AV1(2026-09-19新設)。"-c:v av1"は疑似指定で、Rust側が使えるエンコーダ
  // (libsvtav1優先、無ければlibaom-av1)へ置き換える。出力名は`<名前>.av1.<拡張子>`。
  "av1-mkv": ["-c:v", "av1", "-c:a", "libopus"],
  "av1-webm": ["-c:v", "av1", "-c:a", "libopus"],
  "av1-mp4": ["-c:v", "av1", "-c:a", "aac"],
  // HEVC 10bit(HDR10互換のベースレイヤー)。音声は無変換でサラウンド/Atmosを保持。
  "hevc-hdr10-mkv": ["-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-c:a", "copy"],
  // 全ストリーム無変換コピー(Dolby Vision RPU・Atmos・字幕等を保持)。
  "passthrough-mkv": ["-map", "0", "-c", "copy"],
};

// DSDと同時に作るPCM版(FLAC 24bit / 352.8kHz)。
const DSD_COMPANION_CODEC_ARGS = ["-af", "hq-resample@352800", "-c:a", "flac", "-sample_fmt", "s32", "-bits_per_raw_sample", "24"];

const AUDIO_CODEC_ARGS = {
  mp3: ["-c:a", "libmp3lame"],
  wav: ["-c:a", "pcm_s16le"],
  flac: ["-c:a", "flac"],
  aac: ["-c:a", "aac"],
  ogg: ["-c:a", "libvorbis"],
  opus: ["-c:a", "libopus"],
  // DSD(2026-09-19新設): ffmpegでは書き出せないためRust側の自前変換器(engine::dsd)を使う。
  // 高解像度PCM(R-2R等マルチビットDAC向け、2026-09-19新設): 使える最高品質のリサンプラ+ディザ。
  dxd352: ["-af", "hq-resample@352800", "-c:a", "pcm_s24le"],
  pcm352_32: ["-af", "hq-resample@352800", "-c:a", "pcm_s32le"],
  pcm384_24: ["-af", "hq-resample@384000", "-c:a", "pcm_s24le"],
  pcm384_32: ["-af", "hq-resample@384000", "-c:a", "pcm_s32le"],
  pcm705_32: ["-af", "hq-resample@705600", "-c:a", "pcm_s32le"],
  pcm768_32: ["-af", "hq-resample@768000", "-c:a", "pcm_s32le"],
  dsd64: [],
  dsd128: [],
  dsd256: [],
  dsd512: [],
  dsd1024: [],
  ac3: ["-c:a", "ac3"],
  eac3: ["-c:a", "eac3"],
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
  frameAccurateCheckbox.addEventListener("change", async () => {
    if (frameAccurateCheckbox.checked) {
      // GPUエンコーダの有無はRust側(実際に1フレーム試しエンコード)でしか
      // 判定できないため、ここではCPUの命令セット対応状況(open-cpu検出)
      // だけを見て、GPUが無かった場合にどれくらい遅くなりそうかを事前警告する。
      try {
        const estimate = await invoke("estimate_cpu_encode_speed");
        if (estimate.speed_hint === "slow" || estimate.speed_hint === "very_slow") {
          const proceed = window.confirm(
            `${estimate.message_ja}\n${estimate.message_en}\n\n` +
            "(GPUエンコーダが実際に見つかればこの警告どおりにはなりません / this warning won't apply if a GPU encoder is actually found)\n\n" +
            "このままフレーム精度カットを続けますか？ / Proceed with frame-accurate cutting anyway?"
          );
          if (!proceed) {
            frameAccurateCheckbox.checked = false;
            return;
          }
        }
      } catch (e) {
        // 参考情報の取得失敗は無視してよい(必須機能ではない)。
      }
    }
    file.frameAccurate = frameAccurateCheckbox.checked;
  });
  frameAccurateLabel.append(
    frameAccurateCheckbox,
    document.createTextNode(" フレーム精度で正確にカットする(GPUエンコーダがあれば自動使用、無ければCPU) / Frame-accurate cut (uses GPU encoder if available)")
  );

  container.append(document.createElement("h4"), rangeList, addForm, frameAccurateLabel);
  container.querySelector("h4").textContent = "カットする区間(いくつでも追加可)";
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

// 音楽CD(CD-DA)取り込み
let cddaTracks = [];
document.getElementById("cdda-scan-btn").addEventListener("click", async () => {
  const driveSel = document.getElementById("cdda-drive");
  const listEl = document.getElementById("cdda-tracks");
  try {
    if (!driveSel.options.length) {
      const devices = await invoke("list_burn_devices");
      for (const d of devices) driveSel.add(new Option(d, d));
      if (!devices.length) throw "光学ドライブが見つかりません / No optical drive found";
    }
    cddaTracks = await invoke("list_cd_tracks", { drive: driveSel.value });
  } catch (e) {
    log(`CDを読めません / Cannot read disc: ${e}`);
    return;
  }
  listEl.innerHTML = "";
  for (const t of cddaTracks) {
    const li = document.createElement("li");
    const secs = Math.round(t.sectors / 75);
    const label = `Track ${String(t.number).padStart(2, "0")} — ${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}` + (t.is_audio ? "" : " (データ/data)");
    li.innerHTML = `<label><input type="checkbox" data-track="${t.number}" ${t.is_audio ? "checked" : "disabled"} /> ${label}</label>`;
    listEl.appendChild(li);
  }
  document.getElementById("cdda-rip-btn").disabled = !cddaTracks.some((t) => t.is_audio);
  if (!cddaTracks.some((t) => t.is_audio)) log("音声トラックがありません(データディスクの可能性) / No audio tracks (probably a data disc)");
});

document.getElementById("cdda-rip-btn").addEventListener("click", async () => {
  if (!outputFolder) {
    log("先に出力先フォルダを選んでください / Choose an output folder first");
    return;
  }
  const tracks = [...document.querySelectorAll("#cdda-tracks input[data-track]:checked")].map((c) => Number(c.dataset.track));
  if (!tracks.length) return;
  const btn = document.getElementById("cdda-rip-btn");
  btn.disabled = true;
  log(`取り込み中(${tracks.length}トラック)... / Ripping ${tracks.length} track(s)...`);
  try {
    const outs = await invoke("rip_cd_tracks", { drive: document.getElementById("cdda-drive").value, tracks, outputDir: outputFolder + "/CD-rip", secure: document.getElementById("cdda-secure").checked });
    for (const path of outs) sourceFiles.push({ path, cutRanges: [], frameAccurate: false, editing: false });
    renderFileList();
    log(`取り込み完了 / Ripped ${outs.length} track(s) → ${outputFolder}/CD-rip`);
  } catch (e) {
    log(`取り込み失敗 / Rip failed: ${e}`);
  } finally {
    btn.disabled = false;
  }
});

document.getElementById("pick-output-btn").addEventListener("click", async () => {
  try {
    const dir = await open({ directory: true });
    if (!dir) return;
    outputFolder = dir;
    outputFolderEl.value = dir;
  } catch (e) {
    if (String(e).includes("not implemented on mobile")) {
      // Tauri標準のdialogプラグインはモバイルでのフォルダ選択を未実装
      // なので、Android向けに追加した自前プラグイン(tauri-plugin-android-folder、
      // SAF ACTION_OPEN_DOCUMENT_TREE)にフォールバックする。
      try {
        const uri = await invoke("pick_output_tree");
        if (!uri) return; // ユーザーがキャンセル
        outputFolder = uri;
        outputFolderEl.value = uri;
        log(
          "注意: Androidでは選択したフォルダがcontent:// URIになります。ffmpeg/xorrisoはAndroidに" +
          "同梱されていないため、変換の実行自体は現時点でエラーになります(フォルダ選択UIの動作確認まで)。 / " +
          "Note: on Android the selected folder is a content:// URI. Since ffmpeg/xorriso aren't bundled for " +
          "Android, actually running a conversion will still fail for now — this only verifies the folder-picker UI."
        );
      } catch (e2) {
        log(`エラー: ${e2}`);
      }
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

/** 同時実行数の目安。WebView(素のブラウザAPI)から分かる範囲の
 * `navigator.hardwareConcurrency`を使い、無ければ4を既定にする
 * (2026-09-16新設、ユーザー指示「非同期でマルチスレッドで同時に
 * 行える様に」への対応)。ffmpegプロセス自体もマルチスレッドで動くため
 * 「論理コア数と同じだけ同時起動」は詰め込みすぎになりやすく、半分
 * 程度を上限にする(最低1)。 */
function conversionConcurrency() {
  const cores = typeof navigator !== "undefined" && navigator.hardwareConcurrency ? navigator.hardwareConcurrency : 4;
  return Math.max(1, Math.floor(cores / 2));
}

/** `tasks`(引数無しの非同期関数の配列)を、同時実行数`limit`件までの
 * 並列度で全て実行する(単純なワーカープール、外部ライブラリ不要)。 */
async function runWithConcurrencyLimit(tasks, limit) {
  const results = new Array(tasks.length);
  let next = 0;
  async function worker() {
    while (true) {
      const i = next++;
      if (i >= tasks.length) return;
      results[i] = await tasks[i]();
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, tasks.length) }, worker));
  return results;
}

/** 音声/動画フォーマット×ソースファイルの全組み合わせを変換する。
 * 2026-09-16変更: 逐次(1件ずつawait)ではなく、`runWithConcurrencyLimit`で
 * 複数のffmpegプロセスを同時に起動する非同期・マルチスレッド実行に
 * した(1トラック変換完了を待ってから次へ、という無駄な直列待ちを
 * 無くす——例: 「MP4をWAVへ」+「ISO化」のような組み合わせでも、
 * 複数ファイル・複数フォーマットの変換自体は並列に進む)。 */
/** 解像度指定(2026-09-17新設)。「無指定」はnull(元のまま)、「カスタム」は
 * 入力欄の値、「AI最適化」はソースの解像度を検出し4Kを超える場合のみ
 * 4Kへ抑える簡易ヒューリスティック(意味的な画質判断は行わない、
 * 正直な開示——index.htmlの注記参照)。動画ファイルにのみ適用する。 */
async function resolveResolutionSetting(f) {
  const preset = document.getElementById("resolution-preset").value;
  if (preset === "unspecified") return null;
  if (preset === "custom") {
    const width = parseInt(document.getElementById("resolution-custom-width").value, 10);
    const height = parseInt(document.getElementById("resolution-custom-height").value, 10);
    return width > 0 && height > 0 ? { width, height } : null;
  }
  if (preset === "ai") {
    try {
      const info = await invoke("probe_media", { path: f.path });
      const max4k = 3840 * 2160;
      if (info.width && info.height && info.width * info.height > max4k) {
        const scale = Math.sqrt(max4k / (info.width * info.height));
        return { width: Math.round((info.width * scale) / 2) * 2, height: Math.round((info.height * scale) / 2) * 2 };
      }
      return null; // 4K以下ならそのまま(不要な再エンコードを避ける)
    } catch (e) {
      log(`警告: AI最適化の解像度判定に失敗しました(${f.path}): ${e}`);
      return null;
    }
  }
  const [w, h] = preset.split("x").map((s) => parseInt(s, 10));
  return { width: w, height: h };
}

/** フレームレート指定(2026-09-17新設)。「AI最適化」はソースのfpsを検出し
 * 24/30/60/120のうち最も近い値へ合わせる簡易ヒューリスティック
 * (意味的な動き解析は行わない、正直な開示)。動画ファイルにのみ適用する。 */
async function resolveFpsSetting(f) {
  const preset = document.getElementById("fps-preset").value;
  if (preset === "unspecified") return null;
  if (preset === "custom") {
    const value = parseInt(document.getElementById("fps-custom-value").value, 10);
    return value > 0 ? value : null;
  }
  if (preset === "ai") {
    try {
      const info = await invoke("probe_media", { path: f.path });
      if (!info.fps) return null;
      const standardFps = [24, 30, 60, 120];
      const nearest = standardFps.reduce((a, b) => (Math.abs(b - info.fps) < Math.abs(a - info.fps) ? b : a));
      return Math.abs(nearest - info.fps) < 0.5 ? null : nearest; // 既に標準値に近ければ無変換
    } catch (e) {
      log(`警告: AI最適化のFPS判定に失敗しました(${f.path}): ${e}`);
      return null;
    }
  }
  return parseInt(preset, 10);
}

/** 元素材のDolby Vision/Atmos/サラウンドの検出結果をログに出す(1ファイル1回)。 */
const traitsLogged = new Set();
async function logSourceTraits(f) {
  if (traitsLogged.has(f.path)) return;
  traitsLogged.add(f.path);
  try {
    const info = await invoke("probe_media", { path: f.path });
    const traits = [];
    if (info.dolby_vision) traits.push("Dolby Vision");
    if (info.audio_profile && /atmos/i.test(info.audio_profile)) traits.push("Dolby Atmos");
    else if (info.audio_codec === "truehd") traits.push("TrueHD(Atmosの可能性 / may carry Atmos)");
    if (info.audio_channels && info.audio_channels > 2) traits.push(`ch サラウンド / surround`);
    if (traits.length > 0) {
      log(`検出:  — 。保持するには「無変換コピー」を選択してください。 / Detected: . Choose "Lossless copy" to keep them.`);
    }
  } catch (e) {
    // 検出は参考情報なので失敗しても続行する
  }
}

async function convertAll(formats, codecMap, mode, bitrateKbps) {
  const jobs = [];
  for (const format of formats) {
    const codecArgs = codecMap[format];
    for (const f of sourceFiles) {
      if (f.path.toLowerCase().endsWith(".pdf")) {
        continue; // PDFは上のPDF見開き変換で個別に処理済み、ffmpeg変換の対象外
      }
      const dsdMatch = /^dsd(\d+)$/.exec(format);
      const av1Match = /^(av1|hevc-hdr10|passthrough)-(.+)$/.exec(format);
      const hiresMatch = /^(dxd352|pcm\d+_\d+)$/.exec(format);
      const outputPath = dsdMatch ? `${outputFolder}/${baseName(f.path)}.dsd${dsdMatch[1]}.dsf` : hiresMatch ? `${outputFolder}/${baseName(f.path)}.${format}.wav` : av1Match ? `${outputFolder}/${baseName(f.path)}.${av1Match[1]}.${av1Match[2]}` : `${outputFolder}/${baseName(f.path)}.${format}`;
      jobs.push({ outputPath, f, format, codecArgs });
    }
  }

  // DSD非対応の機器・DACでも聴けるよう、DSDと同時にPCM版(FLAC 24bit/352.8kHz)も作る。
  // ファイル自体にDSD→PCMの自動フォールバック機構は無い(再生機器の機能)ため、両方を並べて出力し、
  // 再生側が使える方を選べるようにする。
  if (codecMap === AUDIO_CODEC_ARGS && formats.some((x) => /^dsd\d+$/.test(x)) && document.getElementById("dsd-companion-pcm").checked) {
    for (const f of sourceFiles) {
      if (f.path.toLowerCase().endsWith(".pdf")) continue;
      jobs.push({ outputPath: `${outputFolder}/${baseName(f.path)}.dsd-companion.flac`, f, format: "dsd-companion", codecArgs: DSD_COMPANION_CODEC_ARGS });
    }
  }

  const tasks = jobs.map(({ outputPath, f, format, codecArgs }) => async () => {
    // (ジョブ生成ループ内の定数はここでは見えないため、formatから再度判定する)
    const dsdMatch = /^dsd(\d+)$/.exec(format);
    const hiresMatch = /^(dxd352|pcm\d+_\d+)$/.exec(format);
    log(`変換中: ${f.path} -> ${outputPath}`);
    await logSourceTraits(f);
    try {
      // 自動/最高品質モードのビットレートは、元ファイル自身のビットレートを
      // 超えても画質は上がらない(尺が極端に短いと容量逆算が数Gbpsになる、
      // 実機で「10003421 kbps」を確認)ため、元のビットレートで頭打ちにする。
      let effectiveBitrateKbps = bitrateKbps;
      if ((mode === "auto" || mode === "max_quality") && format !== "wav" && format !== "flac") {
        try {
          const srcInfo = await invoke("probe_media", { path: f.path });
          if (srcInfo.bit_rate) {
            const srcKbps = Math.floor(srcInfo.bit_rate / 1000);
            if (srcKbps > 0 && srcKbps < effectiveBitrateKbps) {
              log(`ビットレートを元ファイルの${srcKbps} kbpsに制限しました(${effectiveBitrateKbps} kbpsは元より高く無意味なため)。 / Capped bitrate at the source's ${srcKbps} kbps.`);
              effectiveBitrateKbps = srcKbps;
            }
          }
        } catch (e) {
          log(`警告: 元ファイルのビットレート取得に失敗(${f.path}): ${e}`);
        }
      }
      // DSDは非常に大きいため、出力サイズの見積もりを事前に表示し、選択中のディスクに収まらなければ警告する。
      if (dsdMatch) {
        try {
          const info = await invoke("probe_media", { path: f.path });
          const bytes = await invoke("estimate_dsd_size", { multiplier: parseInt(dsdMatch[1], 10), channels: 2, durationSecs: info.duration_secs });
          const gb = (bytes / 1e9).toFixed(2);
          const caps = { cd700: 0.686, dvd47: 4.6, dvd_dl85: 8.3, bd25: 24.5, bd50: 49, bd100: 98, bd128: 125 };
          const smallest = checkedValues("disc-type").map((d) => caps[d]).filter((c) => c).sort((a, b) => a - b)[0];
          const over = smallest && bytes / 1e9 > smallest;
          log(`DSDの推定サイズ:  GB / Estimated DSD size:  GB` + (over ? ` — ⚠ 選択したディスク(約GB)に収まりません / exceeds the selected disc (~ GB)` : ""));
        } catch (e) {
          // 見積もりは参考情報のため失敗しても続行する
        }
      }
      const isVideo = format in VIDEO_CODEC_ARGS;
      const resolution = isVideo ? await resolveResolutionSetting(f) : null;
      const fps = isVideo ? await resolveFpsSetting(f) : null;
      await invoke("convert_media", {
        job: {
          input_path: f.path,
          output_path: outputPath,
          codec_args: codecArgs,
          bitrate: format === "wav" || format === "flac" || hiresMatch || format === "dsd-companion" || dsdMatch ? null : { [mode === "auto" || mode === "max_quality" ? "auto_max_for_capacity" : "fixed"]: effectiveBitrateKbps },
          trim: null,
          cut_ranges: f.cutRanges.length > 0 ? f.cutRanges.map((r) => ({ start_secs: r.startSecs, end_secs: r.endSecs })) : null,
          frame_accurate: f.frameAccurate,
          resolution,
          fps,
          dsd_rate: dsdMatch ? parseInt(dsdMatch[1], 10) : null,
          dop_wav_bits: dsdMatch && document.getElementById("dsd-dop-wav").checked ? parseInt(document.getElementById("dsd-dop-bits").value, 10) : null,
          audio_bwe:
            !isVideo && document.getElementById("audio-bwe").checked
              ? { cutoff_hz: parseFloat(document.getElementById("audio-bwe-cutoff").value) || null }
              : null,
          ai_upscale:
            isVideo && format !== "passthrough-mkv" && document.getElementById("ai-upscale").checked
              ? { model: document.getElementById("ai-upscale-model").value, scale: parseInt(document.getElementById("ai-upscale-scale").value, 10), backend: document.getElementById("ai-upscale-backend").value }
              : null,
          ai_denoise: document.getElementById("ai-denoise").checked && format !== "passthrough-mkv" ? { mix: parseFloat(document.getElementById("ai-denoise-mix").value) } : null,
        },
      });
      log(`完了: ${outputPath}`);
      return outputPath;
    } catch (e) {
      log(`エラー: ${e}`);
      return null;
    }
  });

  const results = await runWithConcurrencyLimit(tasks, conversionConcurrency());
  return results.filter((p) => p !== null);
}

document.getElementById("ai-denoise-mix").addEventListener("input", (e) => {
  document.getElementById("ai-denoise-mix-label").textContent = e.target.value;
});

/** ディスク変換の方向に応じて、選べる解像度プリセットを絞り込む(2026-09-19新設)。
 * BD→DVD: 720x480 / 1920x1080、DVD→BD: 1920x1080 / 3840x2160。
 * 「無指定」「カスタム」「AI最適化」は常に選べる。 */
const DIRECTION_RESOLUTIONS = {
  any: null,
  bd_to_dvd: ["720x480", "720x576", "1920x1080"],
  dvd_to_bd: ["1920x1080", "3840x2160"],
};
function applyDiscDirection(direction) {
  const allowed = DIRECTION_RESOLUTIONS[direction];
  const select = document.getElementById("resolution-preset");
  const fixedValues = ["unspecified", "custom", "ai"];
  for (const opt of select.options) {
    const enabled = allowed === null || fixedValues.includes(opt.value) || allowed.includes(opt.value);
    opt.hidden = !enabled;
    opt.disabled = !enabled;
  }
  if (allowed !== null && select.selectedOptions[0]?.disabled) {
    select.value = allowed[0];
    select.dispatchEvent(new Event("change"));
  }
}
document.getElementById("disc-direction").addEventListener("change", (e) => applyDiscDirection(e.target.value));

document.getElementById("resolution-preset").addEventListener("change", (e) => {
  document.getElementById("resolution-custom-inputs").hidden = e.target.value !== "custom";
  document.getElementById("resolution-ai-hint").hidden = e.target.value !== "ai";
});
document.getElementById("fps-preset").addEventListener("change", (e) => {
  document.getElementById("fps-custom-value").hidden = e.target.value !== "custom";
  document.getElementById("fps-ai-hint").hidden = e.target.value !== "ai";
});

document.getElementById("rebind-pdfs-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  const pdfFiles = sourceFiles.filter((f) => f.path.toLowerCase().endsWith(".pdf"));
  if (pdfFiles.length === 0) {
    log("エラー: PDFファイルをソースに追加してください。 / Error: add at least one PDF to the source list.");
    return;
  }
  if (!outputFolder) {
    log("エラー: 出力先フォルダを選択してください。 / Error: choose an output folder.");
    return;
  }
  log(`${pdfFiles.length}件のPDFの綴じ方向を変換中... / Converting binding direction for ${pdfFiles.length} PDF(s)...`);
  const results = await invoke("rebind_pdfs", { pdfPaths: pdfFiles.map((f) => f.path), outputDir: outputFolder });
  results.forEach((result, i) => {
    if (result && typeof result === "object" && "Ok" in result) {
      log(`完了: ${pdfFiles[i].path} -> ${result.Ok}`);
    } else {
      log(`エラー(${pdfFiles[i].path}): ${result && result.Err ? result.Err : result}`);
    }
  });
  log("すべての処理が完了しました。");
});

document.getElementById("concat-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  const targets = sourceFiles.filter((f) => !f.path.toLowerCase().endsWith(".pdf"));
  if (targets.length < 2) {
    log("エラー: 結合には2つ以上の音声/動画ファイルが必要です。 / Error: concatenation needs at least 2 audio/video files.");
    return;
  }
  if (!outputFolder) {
    log("エラー: 出力先フォルダを選択してください。 / Error: choose an output folder.");
    return;
  }
  const videoExts = Object.keys(VIDEO_CODEC_ARGS);
  const hasVideo = targets.some((f) => videoExts.some((ext) => f.path.toLowerCase().endsWith(`.${ext}`)));
  const outputPath = `${outputFolder}/composite-output.${hasVideo ? "mp4" : "mp3"}`;
  log(`${targets.length}件のファイルをソース順に結合中... / Concatenating ${targets.length} file(s) in list order...`);
  try {
    await invoke("concat_media_files", { inputPaths: targets.map((f) => f.path), outputPath, hasVideo });
    log(`完了: ${outputPath}`);
  } catch (e) {
    log(`エラー: ${e}`);
  }
});

/** 動画/音声の分割(2026-09-16新設)。等間隔かサイズ指定で複数ファイルに
 * 分割する——既存の`convert_media`の`trim`(開始+長さ)をそのまま使うため、
 * 新しい抽出処理は不要。「あまり」(サイズ指定分割で割り切れない最後の
 * 区間)だけディスク容量いっぱいのビットレートへ自動調整することで、
 * ユーザー指示「あまりは、DISKいっぱいにビットレートを自動変更して
 * 自動編集して」に対応する。 */
// 分割数の入力欄: 1は「分けない」(0)と同じなので、矢印で0の次は2、2の前は0へ飛ばす。
{
  const countEl = document.getElementById("split-equal-count");
  let prev = parseInt(countEl.value, 10);
  countEl.addEventListener("change", () => {
    const v = parseInt(countEl.value, 10);
    if (v === 1) countEl.value = String(prev === 0 ? 2 : 0);
    prev = parseInt(countEl.value, 10);
  });
}

document.getElementById("split-btn").addEventListener("click", async () => {
  logEl.textContent = "";
  const target = sourceFiles.find((f) => !f.path.toLowerCase().endsWith(".pdf"));
  if (!target) {
    log("エラー: 分割対象の音声/動画ファイルが見つかりません。 / Error: no audio/video file found to split.");
    return;
  }
  if (!outputFolder) {
    log("エラー: 出力先フォルダを選択してください。 / Error: choose an output folder.");
    return;
  }

  const totalSecs = await effectiveDurationSecs(target);
  if (totalSecs <= 0) {
    log("エラー: 対象ファイルの尺を取得できませんでした。 / Error: could not determine the file's duration.");
    return;
  }

  const splitMode = document.querySelector('input[name="split-mode"]:checked').value;
  const fixedBitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10) || 192;
  let segments;
  let nominalSegmentSecs = null;
  if (splitMode === "equal") {
    const count = parseInt(document.getElementById("split-equal-count").value, 10);
    if (count === 0 || count === 1) {
      log("分割しません(0個・1個は「分けない」と同じです)。2以上を指定すると分割します。 / Not splitting (0 or 1 part means no split). Enter 2 or more to split.");
      return;
    }
    if (!Number.isInteger(count) || count < 2) {
      log("エラー: 分割数は0(分割しない)または2以上の整数にしてください。 / Error: enter 0 (no split) or an integer of 2 or more.");
      return;
    }
    segments = await invoke("calc_equal_interval_segments", { totalSecs, segmentCount: count });
  } else {
    const targetMb = parseFloat(document.getElementById("split-size-mb").value);
    if (!targetMb || targetMb <= 0) {
      log("エラー: 分割サイズ(MB)を入力してください。 / Error: enter a split size in MB.");
      return;
    }
    // 現在のビットレート設定(section 7の固定値)から、目標サイズに相当する区間長(秒)を逆算する。
    nominalSegmentSecs = (targetMb * 1024 * 1024 * 8) / (fixedBitrateKbps * 1000);
    segments = await invoke("calc_fixed_length_segments", { totalSecs, segmentSecs: nominalSegmentSecs });
  }

  if (segments.length === 1) {
    log("分割する必要がありません: ファイル全体が指定した大きさ(または1区間)に収まるため、分割されません。 / Nothing to split: the whole file fits in a single part.");
    return;
  }
  if (segments.length === 0) {
    log("エラー: 分割区間を計算できませんでした。 / Error: could not compute split segments.");
    return;
  }

  const discTypes = checkedValues("disc-type");
  const autoFitRemainder = document.getElementById("split-remainder-disk-fit").checked;
  const ext = target.path.split(".").pop();
  const base = baseName(target.path);

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i];
    const isRemainder = nominalSegmentSecs !== null && i === segments.length - 1 && seg.duration_secs < nominalSegmentSecs - 0.01;
    let bitrateKbpsForSegment = fixedBitrateKbps;
    if (isRemainder && autoFitRemainder && discTypes.length > 0) {
      bitrateKbpsForSegment = await invoke("calc_auto_bitrate_kbps", { disc: discTypes[0], totalDurationSecs: seg.duration_secs, reservedBytes: 50 * 1024 * 1024 });
      log(`あまり区間(${i + 1}/${segments.length}、${discTypes[0]})をディスクいっぱいのビットレート(${bitrateKbpsForSegment} kbps)に自動調整します。 / Auto-fitting the remainder segment (${i + 1}/${segments.length}, ${discTypes[0]}) to ${bitrateKbpsForSegment} kbps to fill the disc.`);
    }
    const outputPath = `${outputFolder}/${base}-part${String(i + 1).padStart(3, "0")}.${ext}`;
    log(`分割中(${i + 1}/${segments.length}): ${outputPath} ...`);
    try {
      await invoke("convert_media", {
        job: {
          input_path: target.path,
          output_path: outputPath,
          codec_args: [],
          bitrate: { fixed: bitrateKbpsForSegment },
          trim: { start_secs: seg.start_secs, duration_secs: seg.duration_secs },
          cut_ranges: null,
          frame_accurate: false,
        },
      });
      log(`完了: ${outputPath}`);
    } catch (e) {
      log(`エラー: ${e}`);
    }
  }
  log("すべての処理が完了しました。");
});

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

  // PDF見開き変換(2026-09-16新設)。音声/動画とは独立して、ソースに
  // 含まれるPDFがあれば見開き画像として先に書き出す。
  const pdfFiles = sourceFiles.filter((f) => f.path.toLowerCase().endsWith(".pdf"));
  if (pdfFiles.length > 0) {
    const binding = document.querySelector('input[name="pdf-binding"]:checked').value;
    for (const f of pdfFiles) {
      const baseName = f.path.replace(/\\/g, "/").split("/").pop().replace(/\.pdf$/i, "");
      const pdfOutDir = `${outputFolder}/pdf-spreads-${baseName}`;
      log(`PDF見開き変換中: ${f.path} ... / Converting PDF spreads: ${f.path} ...`);
      try {
        const outputs = await invoke("convert_pdf_to_spreads", { pdfPath: f.path, outputDir: pdfOutDir, binding, maxDimension: 3840 });
        log(`  ${outputs.length}枚の見開き画像を書き出しました: ${pdfOutDir} / wrote ${outputs.length} spread image(s) to: ${pdfOutDir}`);
      } catch (e) {
        log(`  エラー: ${e}`);
      }
    }
  }

  // ソースが全てPDFの場合(音声/動画の変換対象が無い)は、PDF見開き変換
  // だけで完了とする——音声/動画フォーマット必須のバリデーションを
  // 誤って適用しないようにする。
  if (pdfFiles.length === sourceFiles.length) {
    log("すべての処理が完了しました。");
    return;
  }

  let audioFormats = checkedValues("audio-format");
  let videoFormats = checkedValues("video-format");
  const discTypes = checkedValues("disc-type");
  let wantIso = document.getElementById("output-iso").checked;

  const mode = bitrateMode();

  // 「最高音質・最高画質」モード(2026-09-16新設): フォーマット未選択でも
  // 実行できる——音声フォーマットが1つも選ばれていなければロスレスWAVを
  // 自動選択し、常にISO化する(ユーザー指示「音声や画像フォーマットを
  // 選択しない場合...ロスレスのWAVに変換後ISO変換を同時に行なう」)。
  if (mode === "max_quality") {
    if (discTypes.length === 0) {
      log("エラー: 最高音質・最高画質モードにはディスク種別を1つ以上選択してください。 / Error: select at least one disc type for maximum-quality mode.");
      return;
    }
    if (audioFormats.length === 0 && videoFormats.length === 0) {
      audioFormats = ["wav"];
      log("音声/動画フォーマットが未選択のため、ロスレスWAVを自動選択しました。 / No audio/video format selected — automatically using lossless WAV.");
    }
    wantIso = true;
  }

  if (audioFormats.length === 0 && videoFormats.length === 0) {
    log("エラー: 音声または動画フォーマットを1つ以上選択してください。");
    return;
  }

  let bitrateKbps = parseInt(document.getElementById("bitrate-fixed").value, 10);

  if (mode === "auto" || mode === "max_quality") {
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

  // 最高音質モードでWAV(ロスレス)を使う場合、容量から逆算した
  // 「収まる/収まらない、収まるなら何秒まで」を先に表示する
  // (ユーザー指示「必要な時間やデータサイズを自動で割り出す」)。
  if (mode === "max_quality" && audioFormats.includes("wav")) {
    let totalDuration = 0;
    for (const f of sourceFiles) {
      totalDuration += await effectiveDurationSecs(f);
    }
    for (const discType of discTypes) {
      const est = await invoke("estimate_lossless_audio_fit", {
        disc: discType,
        totalDurationSecs: totalDuration,
        reservedBytes: 50 * 1024 * 1024,
      });
      const { h, m, sec } = secsToHms(totalDuration);
      const need = `${h}時間${m}分${Math.floor(sec)}秒 / ${h}h${m}m${Math.floor(sec)}s`;
      if (est.fits) {
        log(`[${discType}] 収録時間(${need})はロスレスWAVで収まります(必要: ${(est.required_bytes / 1e6).toFixed(1)}MB / 容量: ${(est.usable_bytes / 1e6).toFixed(1)}MB)。`);
      } else {
        const fit = secsToHms(est.max_fitting_duration_secs);
        log(
          `⚠️ [${discType}] 収録時間(${need})はロスレスWAVでは収まりません(必要: ${(est.required_bytes / 1e9).toFixed(2)}GB / 容量: ${(est.usable_bytes / 1e9).toFixed(2)}GB)。` +
            `このディスクにロスレスで収まるのは最大${fit.h}時間${fit.m}分${Math.floor(fit.sec)}秒までです。 / ` +
            `Won't fit losslessly — this disc can hold at most ${fit.h}h${fit.m}m${Math.floor(fit.sec)}s of lossless audio.`
        );
      }
    }
  }

  // ── 収まらない分の自動調整方法(2026-09-16新設) ──────────────────
  // ユーザー指示「サイズか、時分秒か、ディスクいっぱいか、AI判断で
  // 自動カットのいずれかを選択可能」への対応。「ディスクいっぱい」
  // (既定)は上のbitrate-modeの挙動をそのまま使うため、ここでは
  // それ以外の3つだけを扱う。
  const fitStrategy = document.querySelector('input[name="fit-strategy"]:checked').value;

  if (fitStrategy === "target_size") {
    const targetMb = parseFloat(document.getElementById("fit-target-size-mb").value);
    if (!targetMb || targetMb <= 0) {
      log("エラー: 目標サイズ(MB)を入力してください。 / Error: enter a target size in MB.");
      return;
    }
    let totalDuration = 0;
    for (const f of sourceFiles) {
      totalDuration += await effectiveDurationSecs(f);
    }
    bitrateKbps = await invoke("calc_bitrate_for_target_size_kbps", {
      targetBytes: Math.round(targetMb * 1024 * 1024),
      totalDurationSecs: totalDuration,
    });
    log(`サイズ指定モード: 目標${targetMb}MBに収めるためのビットレートを算出しました: ${bitrateKbps} kbps / Target-size mode: computed ${bitrateKbps} kbps to fit ${targetMb}MB.`);
  } else if (fitStrategy === "target_duration") {
    const targetSecs = hmsToSecs(document.getElementById("fit-duration-h").value, document.getElementById("fit-duration-m").value, document.getElementById("fit-duration-s").value);
    if (!targetSecs || targetSecs <= 0) {
      log("エラー: 目標の時間(時分秒)を入力してください。 / Error: enter a target duration.");
      return;
    }
    for (const f of sourceFiles) {
      if (f.cutRanges.length === 0) {
        f.cutRanges = [{ startSecs: targetSecs, endSecs: null }];
      }
    }
    log(`時間指定モード: 先頭から${targetSecs}秒までに自動トリムしました(既に編集済みのファイルは変更していません)。 / Target-duration mode: auto-trimmed to the first ${targetSecs}s (files with existing manual edits were left untouched).`);
  } else if (fitStrategy === "ai_auto_cut") {
    // 「AI判断」の正直な開示: 実際にはffmpegの音量ベースの無音検出
    // (silencedetect)による近似であり、意味的なシーン解析ではない
    // (index.htmlの注記・convert::detect_silence_ranges参照)。
    for (const f of sourceFiles) {
      if (f.cutRanges.length > 0) {
        continue; // 既に手動編集済みのファイルは上書きしない
      }
      log(`無音区間を検出中: ${f.path} ... / Detecting silence in: ${f.path} ...`);
      try {
        const silences = await invoke("detect_silence_ranges", { path: f.path, silenceThresholdDb: -30.0, minSilenceSecs: 0.5 });
        if (silences.length > 0) {
          f.cutRanges = silences.map((s) => ({ startSecs: s.start_secs, endSecs: s.end_secs }));
          log(`  ${silences.length}箇所の無音区間を自動カット対象にしました。 / marked ${silences.length} silent range(s) for auto-cut.`);
        } else {
          log(`  無音区間は見つかりませんでした(カット無し)。 / no silence found (nothing to cut).`);
        }
      } catch (e) {
        log(`  エラー: ${e}`);
      }
    }
  }

  // どの形式が選択されているかを実行前に必ず表示する(プリセットボタン等で意図せず選択されていた場合に気づけるように)。
  log(`選択中の出力形式 / Selected formats: 音声=${audioFormats.join(", ") || "なし"} / 動画=${videoFormats.join(", ") || "なし"}`);

  // 音声変換・動画変換もお互いを待たず並行して進める(2026-09-16変更、
  // 「MP4をWAVに変換しつつISO化」のような組み合わせも含め、全体として
  // 非同期・マルチスレッドに実行する)。
  const [audioOutputs, videoOutputs] = await Promise.all([
    audioFormats.length > 0 ? convertAll(audioFormats, AUDIO_CODEC_ARGS, mode, bitrateKbps) : Promise.resolve([]),
    videoFormats.length > 0 ? convertAll(videoFormats, VIDEO_CODEC_ARGS, mode, bitrateKbps) : Promise.resolve([]),
  ]);
  const convertedPaths = [...audioOutputs, ...videoOutputs];

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
          const speedMode = document.getElementById("write-speed-mode").value;
          const speed =
            speedMode === "fixed"
              ? { fixed: parseInt(document.getElementById("write-speed-fixed").value, 10) }
              : speedMode === "max"
                ? "max"
                : "auto";

          // 2026-09-16変更: 複数のディスク種別を選択した場合、物理
          // ドライブが複数あればドライブごとに並行して書き込む
          // (1台のドライブへ同時に2つの書き込みストリームは送れない
          // ため、同じドライブへ割り当てられた種別同士は順番に、
          // 異なるドライブへの書き込みは互いを待たずに並行実行する
          // ——ユーザー指示「書き込みも同時に行なって」への対応)。
          // ドライブより種別数が多い場合はラウンドロビンで割り当てる。
          // ISOがディスクの実用容量に収まらない種別は書き込みを試みず、日英で理由を示して飛ばす
          // (以前は容量超過でも書き込みを始めて途中で失敗していた)。
          let isoBytes = 0;
          try {
            isoBytes = await invoke("folder_size_bytes", { path: outputFolder });
          } catch (e) {
            log(`警告: 出力フォルダのサイズを確認できませんでした: ${e}`);
          }
          const burnable = [];
          for (const discType of discTypes) {
            const usable = await invoke("disc_usable_bytes", { disc: discType });
            if (isoBytes > usable) {
              log(`⚠ [${discType}] 出力が大きすぎてこのディスクには収まりません(${(isoBytes / 1e9).toFixed(2)}GB > 実用容量${(usable / 1e9).toFixed(2)}GB)。書き込みをスキップします。 / Output (${(isoBytes / 1e9).toFixed(2)} GB) exceeds this disc's usable capacity (${(usable / 1e9).toFixed(2)} GB); skipping the burn.`);
            } else {
              burnable.push(discType);
            }
          }
          const byDevice = new Map();
          burnable.forEach((discType, i) => {
            const device = devices[i % devices.length];
            if (!byDevice.has(device)) byDevice.set(device, []);
            byDevice.get(device).push(discType);
          });
          if (devices.length < burnable.length) {
            log(`ドライブが${devices.length}台のため、一部のディスク種別は同じドライブへ順番に書き込みます。`);
          }

          await Promise.all(
            Array.from(byDevice.entries()).map(async ([device, types]) => {
              for (const discType of types) {
                log(`書き込み中(${discType}): ${isoPath} -> ${device}`);
                try {
                  await invoke("burn_image", { imagePath: isoPath, device, disc: discType, speed });
                  log(`書き込み完了(${discType})。`);
                } catch (e) {
                  log(`エラー(${discType}): ${e}`);
                }
              }
            })
          );
        }
      }
    } catch (e) {
      log(`エラー: ${e}`);
    }
  }

  log("すべての処理が完了しました。");
});

// ── 自動アップデート確認(2026-09-16新設) ──────────────────────────
// アプリ起動時に一度だけGitHub Releasesの最新版を確認し、新しいバージョンが
// あれば日本語・英語併記のダイアログで確認してから更新する。デスクトップ
// のみ対応(`window.__TAURI__.updater`はAndroid/iOSでは登録していない
// ため未定義——モバイルはストア/APKサイドロードでの更新が前提、
// `lib.rs`のコメント参照)。
async function checkForUpdatesOnStartup() {
  const updater = window.__TAURI__.updater;
  const process = window.__TAURI__.process;
  if (!updater || !process) {
    return; // モバイル等、アップデータープラグインが登録されていない環境
  }

  let update;
  try {
    update = await updater.check();
  } catch (e) {
    console.warn("update check failed / アップデート確認に失敗しました:", e);
    return;
  }
  if (!update) {
    return; // 最新版を使用中
  }

  const message =
    `新しいバージョン ${update.version} があります。バージョンアップしますか？\n\n` +
    `A new version (${update.version}) is available. Would you like to update now?`;
  const shouldUpdate = window.confirm(message);
  if (!shouldUpdate) {
    return;
  }

  try {
    log(`アップデートをダウンロード中... / Downloading update... (v${update.version})`);
    await update.downloadAndInstall();
    log("アップデートが完了しました。アプリを再起動します。 / Update installed. Restarting the app.");
    await process.relaunch();
  } catch (e) {
    log(`アップデートに失敗しました / Update failed: ${e}`);
  }
}

checkForUpdatesOnStartup();

// 起動時にrs-ffmpeg/rs-xorrisoプラグインの状態(版・同期結果)を表示する(2026-09-19新設)。
// 同じ版が既に入っていれば再コピーしない(上書きの無駄を省く)。
(async () => {
  try {
    const plugins = await invoke("list_plugins");
    const labels = { installed: "新規インストール", updated: "更新", up_to_date: "最新(上書きスキップ)", not_bundled: "未同梱" };
    const shown = plugins.filter((p) => p.action !== "not_bundled");
    if (shown.length > 0) {
      log("プラグイン / Plugins: " + shown.map((p) => `${p.name} [${labels[p.action] ?? p.action}]`).join(", "));
    }
  } catch (e) {
    // 参考情報のため失敗しても続行する
  }
})();

// 起動時に、AI超解像(CPU版)が使う計算カーネル(open-cpuの検出結果)を表示する。
(async () => {
  try {
    const kernel = await invoke("ai_upscale_cpu_kernel");
    log(`AI超解像(CPU版)の計算カーネル / CPU upscaling kernel: ${kernel}`);
  } catch (e) {
    // 参考情報のため失敗しても続行する
  }
})();

// SACD/Blu-rayオーディオ風プリセット(2026-09-19新設): DSD256 + 384kHz/32bit PCM + ISO出力を一括で選ぶ。
// 標準のSACD/BD-Audio規格ディスクではなく、DSF/WAVを収めたデータディスクを作る(index.htmlの注記参照)。
document.getElementById("preset-hires-disc-btn").addEventListener("click", () => {
  for (const name of ["audio-format"]) {
    for (const el of document.querySelectorAll(`input[name="${name}"]`)) {
      el.checked = el.value === "dsd256" || el.value === "pcm384_32";
    }
  }
  document.getElementById("output-iso").checked = true;
  document.getElementById("dsd-companion-pcm").checked = false; // 多くの再生ソフトはDSDを自動でPCM変換して再生でき、容量も倍近く使うため既定では付けない
  log("プリセットを設定しました: DSD256 + 384kHz/32bit PCM + ISO。書き込むディスク種別(6)を選び、実行してください。 / Preset applied: DSD256 + 384 kHz/32-bit PCM + ISO. Pick the disc types (6) and run.");
});
