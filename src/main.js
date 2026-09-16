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
async function convertAll(formats, codecMap, mode, bitrateKbps) {
  const jobs = [];
  for (const format of formats) {
    const codecArgs = codecMap[format];
    for (const f of sourceFiles) {
      const outputPath = `${outputFolder}/${baseName(f.path)}.${format}`;
      jobs.push({ outputPath, f, format, codecArgs });
    }
  }

  const tasks = jobs.map(({ outputPath, f, format, codecArgs }) => async () => {
    log(`変換中: ${f.path} -> ${outputPath}`);
    try {
      await invoke("convert_media", {
        job: {
          input_path: f.path,
          output_path: outputPath,
          codec_args: codecArgs,
          bitrate: format === "wav" || format === "flac" ? null : { [mode === "auto" || mode === "max_quality" ? "auto_max_for_capacity" : "fixed"]: bitrateKbps },
          trim: null,
          cut_ranges: f.cutRanges.length > 0 ? f.cutRanges.map((r) => ({ start_secs: r.startSecs, end_secs: r.endSecs })) : null,
          frame_accurate: f.frameAccurate,
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
          const byDevice = new Map();
          discTypes.forEach((discType, i) => {
            const device = devices[i % devices.length];
            if (!byDevice.has(device)) byDevice.set(device, []);
            byDevice.get(device).push(discType);
          });
          if (devices.length < discTypes.length) {
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
