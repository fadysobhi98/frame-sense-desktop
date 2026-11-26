import { useEffect, useMemo, useState } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Maximize2, Minimize2, Aperture, ArrowDownToLine, Bell, CheckCircle2, ChevronRight, CircleHelp,
  Clapperboard, Command, Cpu, FolderOpen, Gauge, HardDrive, Image,
  Layers3, LayoutDashboard, Palette, Plus, RefreshCw, ScanFace, Settings2,
  ShieldCheck, Smartphone, Sparkles, Upload, WandSparkles, Wifi, XCircle
} from "lucide-react";
import frameSenseLogo from "../frame-sense-logo.png.png";
type Page =
  | "Dashboard" | "Projects & Import" | "Processing Center" | "Photo Gallery"
  | "Quality Review" | "AI Creative Studio" | "Collage Studio"
  | "Export & Delivery" | "Device Center" | "Settings & AI Engine";

type CreateProjectResult = {
  project_id: string;
  project_name: string;
  project_path: string;
};

type ImportResult = {
  imported: number;
  duplicates: number;
  skipped_unsupported: number;
  camera_id: string;
  destination: string;
};

type AutoCorrectProgress = {
  current: number;
  total: number;
  file: string;
  processed: number;
  failed: number;
};

type AutoCorrectReport = {
  total: number;
  processed: number;
  already_done: number;
  failed: { file: string; error: string }[];
  output_dir: string;
  stopped: boolean;
  counts: { regular: number; couple: number; group: number; rejected: number };
};

type RawEngineStatus = {
  available: boolean;
  version: string | null;
  path: string | null;
  error: string | null;
};

const nav: { name: Page; icon: typeof LayoutDashboard }[] = [
  { name: "Dashboard", icon: LayoutDashboard },
  { name: "Projects & Import", icon: FolderOpen },
  { name: "Processing Center", icon: Gauge },
  { name: "Photo Gallery", icon: Image },
  { name: "Quality Review", icon: ShieldCheck },
  { name: "AI Creative Studio", icon: Sparkles },
  { name: "Collage Studio", icon: Layers3 },
  { name: "Export & Delivery", icon: ArrowDownToLine },
  { name: "Device Center", icon: Smartphone },
  { name: "Settings & AI Engine", icon: Settings2 }
];

const plannedCopy: Record<Page, string> = {
  "Dashboard": "",
  "Projects & Import": "",
  "Processing Center": "",
  "Photo Gallery": "RAW decoding, previews, filtering and non-destructive edit controls are planned. Original files remain untouched.",
  "Quality Review": "AI Smart Review & Repair is not connected yet. This screen is reserved for human approval after real analysis is integrated.",
  "AI Creative Studio": "Local AI model integration and Creative Set generation are planned. No cloud AI is required by the approved specification.",
  "Collage Studio": "Collage composition and export are planned. This screen is not a functioning editor yet.",
  "Export & Delivery": "Production JPEG 8-bit export, ICC validation, metadata and ZIP delivery are planned.",
  "Device Center": "QR pairing, encrypted transfer, remote gallery and device permissions are planned.",
  "Settings & AI Engine": "AI model version lock, Smart Power, Smart Storage and performance controls are planned."
};

function App() {
  const [page, setPage] = useState<Page>("Dashboard");
  const [theme, setTheme] = useState<"graphite" | "obsidian">("graphite");
  const [clientName, setClientName] = useState("");
  const [eventType, setEventType] = useState("Wedding");
  const [eventDate, setEventDate] = useState(new Date().toISOString().slice(0, 10));
  const [baseDirectory, setBaseDirectory] = useState("");
  const [sourcePath, setSourcePath] = useState("");
  const [cameraId, setCameraId] = useState("Camera 1");
  const [projectPath, setProjectPath] = useState("");
  const [projectName, setProjectName] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  const [destinationDir, setDestinationDir] = useState("");
  const [folderSource, setFolderSource] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [quality, setQuality] = useState(90);
  const [look, setLook] = useState("auto");
  const TONES: { id: string; label: string; hint: string }[] = [
    { id: "auto", label: "Auto correction + colouring", hint: "Default. Every photo is analysed on its own: exposure, light, white balance, skin tone, warm colouring. Rule-based, no preset look." },
    { id: "natural", label: "Auto correction only", hint: "Same per-photo correction, no colouring or warm tint" },
    { id: "warm", label: "Warm Golden", hint: "Gentle golden warmth, same for every photo" },
    { id: "golden", label: "Golden+", hint: "Strong orange-gold glow" },
    { id: "peach", label: "Peach-Gold", hint: "Soft pink-peach glow, creamy highlights" },
    { id: "amber", label: "Amber Indoor", hint: "Deep amber, warm shadows too" },
    { id: "airy", label: "Bright Airy", hint: "Light, lifted, soft wedding look" },
    { id: "cinematic", label: "Matte Film", hint: "Flat faded matte, muted colours" },
    { id: "cool", label: "Cool Clean", hint: "Cooler blue-ish whites, crisp" },
    { id: "punchy", label: "High Contrast", hint: "Strong contrast and rich colour" },
    { id: "style0", label: "Vintage Warm", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style1", label: "Vintage Sepia", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style2", label: "Vintage Cool", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style3", label: "Vintage Teal-Green", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style4", label: "Vintage Rose", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style5", label: "Vintage Amber", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style6", label: "Retro 70s", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style7", label: "Polaroid", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style8", label: "Kodak Gold", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style9", label: "Old Film", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style10", label: "Washed Vintage", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style11", label: "Vintage Peach", hint: "Optional vintage style (colour maths, not AI)" },
    { id: "style12", label: "Faded Warm", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style13", label: "Faded Cool", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style14", label: "Faded Pink", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style15", label: "Faded Teal-Green", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style16", label: "Faded Lavender", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style17", label: "Faded Peach", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style18", label: "Faded Sage", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style19", label: "Faded Sand", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style20", label: "Faded Blue", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style21", label: "Soft Pastel", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style22", label: "Dusty Rose", hint: "Optional faded style (colour maths, not AI)" },
    { id: "style23", label: "Sunset Fade", hint: "Optional faded style (colour maths, not AI)" },
  ];
  const [adj, setAdj] = useState({ contrast: 0, brightness: 0, warmth: 0, fade: 0, saturation: 0 });
  useEffect(() => {
    invoke("set_adjust", { contrast: adj.contrast, brightness: adj.brightness, warmth: adj.warmth, fade: adj.fade, saturation: adj.saturation }).catch(() => {});
  }, [adj]);
  const [bw, setBw] = useState(true);
  const [toneSuggestion, setToneSuggestion] = useState<{ tone: string; reason: string; probed: number } | null>(null);
  const [toneSuggestBusy, setToneSuggestBusy] = useState(false);
  const [retouch, setRetouch] = useState(true);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [preview, setPreview] = useState<{ before: string; after: string; name: string; path: string } | null>(null);
  const [acProgress, setAcProgress] = useState<AutoCorrectProgress | null>(null);
  const [acReport, setAcReport] = useState<AutoCorrectReport | null>(null);
  const [acBusy, setAcBusy] = useState(false);
  const [acPaused, setAcPaused] = useState(false);
  const [engineStatus, setEngineStatus] = useState<RawEngineStatus | null>(null);

useEffect(() => {
  invoke<RawEngineStatus>("raw_engine_status").then(setEngineStatus).catch(() => setEngineStatus(null));
}, []);

useEffect(() => {
  setToneSuggestion(null);
  if (!folderSource) return;
  setToneSuggestBusy(true);
  invoke<{ tone: string; reason: string; probed: number }>("suggest_session_tone", { folderPath: folderSource })
    .then(setToneSuggestion)
    .catch(() => setToneSuggestion(null))
    .finally(() => setToneSuggestBusy(false));
}, [folderSource]);
const [isFullscreen, setIsFullscreen] = useState(false);

useEffect(() => {
  const appWindow = getCurrentWindow();
  let mounted = true;

  appWindow.isFullscreen().then((value) => {
    if (mounted) setIsFullscreen(value);
  }).catch(console.error);

  const handleKeyDown = async (event: KeyboardEvent) => {
    try {
      if (event.key === "F11") {
        event.preventDefault();
        const next = !(await appWindow.isFullscreen());
        await appWindow.setFullscreen(next);
        if (mounted) setIsFullscreen(next);
      }

      if (event.key === "Escape") {
        await appWindow.setFullscreen(false);
        if (mounted) setIsFullscreen(false);
      }
    } catch (error) {
      console.error(error);
    }
  };

  window.addEventListener("keydown", handleKeyDown);

  return () => {
    mounted = false;
    window.removeEventListener("keydown", handleKeyDown);
  };
}, []);

const toggleFullscreen = async () => {
  try {
    const appWindow = getCurrentWindow();
    const next = !(await appWindow.isFullscreen());
    await appWindow.setFullscreen(next);
    setIsFullscreen(next);
  } catch (error) {
    console.error(error);
  }
};
  function finishAutoCorrect(report: AutoCorrectReport) {
    setAcReport(report);
    setAcPaused(false);
    if (report.stopped) {
      setMessage(`Stopped: ${report.processed} corrected before stopping. Run again to continue the rest.`);
      return;
    }
    if (report.total === 0) {
      setMessage("Found 0 photos in that folder. The app looks for RAW and JPEG photos, including inside subfolders.");
    } else {
      const c = report.counts;
      setMessage(`Auto-correction finished: ${report.processed} corrected and sorted (${c.regular} regular, ${c.couple} couple, ${c.group} group, ${c.rejected} rejected), ${report.already_done} already done, ${report.failed.length} failed.`);
    }
  }

  async function startFolderCorrect() {
    setError(""); setMessage(""); setAcReport(null);
    setAcBusy(true);
    setAcPaused(false);
    setAcProgress({ current: 0, total: 0, file: "", processed: 0, failed: 0 });
    const channel = new Channel<AutoCorrectProgress>();
    channel.onmessage = (p) => setAcProgress(p);
    try {
      const report = await invoke<AutoCorrectReport>("auto_correct_folder", {
        folderPath: folderSource, outputDir: outputDir.trim() || null, quality, look, bw, retouch, onProgress: channel
      });
      finishAutoCorrect(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setAcBusy(false);
    }
  }

  async function previewRetouch() {
    setError("");
    try {
      const picked = await openDialog({
        multiple: false,
        filters: [{ name: "Photos", extensions: ["jpg", "jpeg", "nef", "cr2", "cr3", "arw", "dng", "rw2", "orf", "raf", "pef", "srw"] }],
      });
      if (!picked || typeof picked !== "string") return;
      setPreviewBusy(true);
      setPreview(null);
      const [before, after] = await invoke<[string, string]>("retouch_preview", { photoPath: picked, look, retouch });
      setPreview({ before, after, name: picked.split(/[\\/]/).pop() || picked, path: picked });
    } catch (e) {
      setError(String(e));
    } finally {
      setPreviewBusy(false);
    }
  }

  async function previewTone(toneId: string) {
    if (!preview?.path) return;
    setLook(toneId);
    setPreviewBusy(true);
    try {
      const [before, after] = await invoke<[string, string]>("retouch_preview", { photoPath: preview.path, look: toneId, retouch });
      setPreview({ ...preview, before, after });
    } catch (e) {
      setError(String(e));
    } finally {
      setPreviewBusy(false);
    }
  }

  async function startAutoCorrect() {
    setError(""); setMessage(""); setAcReport(null);
    setAcBusy(true);
    setAcPaused(false);
    setAcProgress({ current: 0, total: 0, file: "", processed: 0, failed: 0 });
    const channel = new Channel<AutoCorrectProgress>();
    channel.onmessage = (p) => setAcProgress(p);
    try {
      const report = await invoke<AutoCorrectReport>("auto_correct_camera", {
        projectPath, cameraId, outputDir: outputDir.trim() || null, quality, look, bw, retouch, onProgress: channel
      });
      finishAutoCorrect(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setAcBusy(false);
    }
  }

  const currentIcon = useMemo(() => nav.find((n) => n.name === page)?.icon ?? LayoutDashboard, [page]);
  const CurrentIcon = currentIcon;

  async function createProject() {
    setError(""); setMessage(""); setBusy(true);
    try {
      const result = await invoke<CreateProjectResult>("create_project", {
        baseDirectory, clientName, eventType, eventDate
      });
      setProjectPath(result.project_path);
      setProjectName(result.project_name);
      setMessage(`Project created: ${result.project_name}`);
      setPage("Projects & Import");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importFiles() {
    setError(""); setMessage(""); setImportResult(null); setBusy(true);
    try {
      const result = await invoke<ImportResult>("import_camera_files", {
        projectPath, sourcePath, cameraId,
        destinationDir: destinationDir.trim() || null
      });
      setImportResult(result);
      setMessage(`Import finished for ${result.camera_id}.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={`app-shell theme-${theme}`}>
      <aside className="sidebar">
        <div className="brand">
<div className="brand-mark">
  <img src={frameSenseLogo} alt="Frame Sense" />
</div>   
       <div><strong>FRAME SENSE</strong><small>AI PHOTOGRAPHY SUITE</small></div>
        </div>
        <div className="workspace-label">WORKSPACE</div>
        <nav>
          {nav.map((item) => {
            const Icon = item.icon;
            return (
              <button key={item.name} className={`nav-item ${page === item.name ? "active" : ""}`} onClick={() => setPage(item.name)}>
                <Icon size={17} /><span>{item.name}</span>
                {page === item.name && <ChevronRight size={15} className="nav-chevron" />}
              </button>
            );
          })}
        </nav>
        <div className="sidebar-bottom">
          <div className="local-pill"><span className="pulse-dot" /> LOCAL WORKSPACE</div>
          <div className="sidebar-foot"><ShieldCheck size={14} /> Photos stay on this computer</div>
        </div>
      </aside>

      <main className="main-area">
        <header className="topbar">
          <div className="breadcrumbs"><span>Frame Sense</span><ChevronRight size={14} /><strong>{page}</strong></div>
          <div className="top-actions">
            <span className="engine-status"><span className="status-dot" /> Foundation Preview</span>
            <button
  className="icon-button"
  title={isFullscreen ? "Exit Fullscreen" : "Enter Fullscreen"}
  onClick={toggleFullscreen}
>
  {isFullscreen ? <Minimize2 size={17} /> : <Maximize2 size={17} />}
</button>
            <button className="icon-button" title="Notifications"><Bell size={17} /></button>
            <button className="profile-button">FS</button>
          </div>
        </header>

        <div className="content">
          <div className="page-heading">
            <div>
              <div className="eyebrow">FRAME SENSE / WORKSPACE</div>
              <h1>{page}</h1>
              <p>{page === "Dashboard" ? "Your local photography workflow, organized and protected." : "A structured foundation for the approved Frame Sense workflow."}</p>
            </div>
            <div className="theme-switch">
              <Palette size={15} />
              <button className={theme === "graphite" ? "selected" : ""} onClick={() => setTheme("graphite")}>Graphite</button>
              <button className={theme === "obsidian" ? "selected" : ""} onClick={() => setTheme("obsidian")}>Obsidian</button>
            </div>
          </div>

          {message && <div className="notice success"><CheckCircle2 size={17} />{message}<button onClick={() => setMessage("")}><XCircle size={15}/></button></div>}
          {error && <div className="notice error"><XCircle size={17} />{error}<button onClick={() => setError("")}><XCircle size={15}/></button></div>}

          {page === "Dashboard" && (
            <>
              <section className="hero-panel">
                <div className="hero-copy">
                  <div className="hero-kicker"><Sparkles size={14}/> LOCAL-FIRST PHOTO WORKFLOW</div>
                  <h2>Every frame.<br/><em>In its right place.</em></h2>
                  <p>One active order at a time. Two independent camera lanes. Original RAW files stay untouched.</p>
                  <button className="primary-button" onClick={() => setPage("Projects & Import")}><Plus size={16}/> Create a project</button>
                </div>
                <div className="hero-orbit"><div className="orbit orbit-one"/><div className="orbit orbit-two"/><div className="orbit-core"><Aperture size={48}/></div><span className="orbit-tag tag-a">RAW SAFE</span><span className="orbit-tag tag-b">LOCAL AI</span></div>
              </section>

              <div className="stats-grid">
                <div className="stat-card"><div className="stat-icon"><FolderOpen size={18}/></div><span>Current project</span><strong>{projectName || "No project yet"}</strong><small>{projectPath || "Create a project to begin"}</small></div>
                <div className="stat-card"><div className="stat-icon"><Cpu size={18}/></div><span>Processing policy</span><strong>One active order</strong><small>Camera 1 + Camera 2 independent</small></div>
                <div className="stat-card"><div className="stat-icon"><HardDrive size={18}/></div><span>Original files</span><strong>RAW preserved</strong><small>No source modification by import</small></div>
                <div className="stat-card"><div className="stat-icon"><ShieldCheck size={18}/></div><span>Export target</span><strong>JPEG · 8-bit</strong><small>sRGB default · Adobe RGB optional</small></div>
              </div>

              <div className="section-title"><div><h3>Start here</h3><p>Only the foundation actions below are connected in this starter.</p></div></div>
              <div className="quick-grid">
                <button className="quick-card" onClick={() => setPage("Projects & Import")}><FolderOpen/><strong>Projects & Import</strong><span>Create project and safely copy card files</span><ChevronRight/></button>
                <button className="quick-card" onClick={() => setPage("Processing Center")}><Gauge/><strong>Processing Center</strong><span>Reserved for the single-order queue</span><ChevronRight/></button>
                <button className="quick-card" onClick={() => setPage("Settings & AI Engine")}><WandSparkles/><strong>AI Engine</strong><span>Local model manager planned</span><ChevronRight/></button>
              </div>
            </>
          )}

          {page === "Projects & Import" && (
            <div className="two-column">
              <section className="panel">
                <div className="panel-heading"><div><span className="step-tag">01 / PROJECT</span><h3>Create a project</h3><p>Creates the project root, camera folders and local SQLite metadata.</p></div><FolderOpen/></div>
                <label>Client name<input value={clientName} onChange={(e) => setClientName(e.target.value)} placeholder="Client name"/></label>
                <label>Event type<input value={eventType} onChange={(e) => setEventType(e.target.value)} placeholder="Wedding / Event"/></label>
                <label>Event date<input type="date" value={eventDate} onChange={(e) => setEventDate(e.target.value)}/></label>
<label>Parent folder on this computer</label>

<div style={{ display: "flex", gap: 8, alignItems: "center" }}>
  <input
    value={baseDirectory}
    readOnly
    placeholder="Choose a parent folder..."
    style={{ flex: 1, minWidth: 0 }}
  />

  <button
    type="button"
    className="secondary-button"
    onClick={async () => {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "Choose project parent folder",
      });

      if (typeof selected === "string") {
        setBaseDirectory(selected);
      }
    }}
  >
    <FolderOpen size={16} />
    Browse
  </button>
</div>                <button className="primary-button full" disabled={busy || !clientName.trim() || !baseDirectory.trim()} onClick={createProject}>{busy ? <RefreshCw className="spin" size={16}/> : <Plus size={16}/>} Create local project</button>
                <p className="field-note">Choose an existing parent folder. The app creates a new project folder inside it.</p>
              </section>

              <section className="panel">
                <div className="panel-heading"><div><span className="step-tag">02 / CARD IMPORT</span><h3>Import to a camera</h3><p>Byte-copy import with SHA-256 verification. Source files are not edited.</p></div><Upload/></div>
                <label>Project folder<input value={projectPath} onChange={(e) => setProjectPath(e.target.value)} placeholder="Create a project first, or paste its path"/></label>
                <label>Camera destination<select value={cameraId} onChange={(e) => setCameraId(e.target.value)}><option>Camera 1</option><option>Camera 2</option></select></label>
                <label>Card/source folder path<input value={sourcePath} onChange={(e) => setSourcePath(e.target.value)} placeholder="Example: E:\"/></label>
                <button className="secondary-button full" disabled={busy || !projectPath.trim() || !sourcePath.trim()} onClick={importFiles}>{busy ? <RefreshCw className="spin" size={16}/> : <ArrowDownToLine size={16}/>} Copy files safely</button>
                {importResult && <div className="import-summary"><strong>Import report</strong><span>Imported: {importResult.imported}</span><span>Duplicates skipped: {importResult.duplicates}</span><span>Unsupported skipped: {importResult.skipped_unsupported}</span><small>{importResult.destination}</small></div>}
                <p className="field-note">This starter copies supported photo/RAW file bytes only. It does not decode RAW or start AI processing.</p>
              </section>
            </div>
          )}

          {page === "Processing Center" && (
            <div className="two-column">
              <section className="panel">
                <div className="panel-heading"><div><span className="step-tag">AUTO-CORRECT A FOLDER</span><h3>Smart Basic auto-correction</h3><p>Pick any folder of photos. Every RAW and JPEG inside (including subfolders) gets automatic exposure and contrast correction. Not AI. Originals stay untouched; corrected 8-bit JPEGs go to the output folder.</p></div><WandSparkles/></div>
                <label>Photo folder</label>
                <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <input
                    value={folderSource}
                    readOnly
                    placeholder="Choose a folder of photos..."
                    style={{ flex: 1, minWidth: 0 }}
                  />
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={async () => {
                      const selected = await openDialog({
                        directory: true,
                        multiple: false,
                        title: "Choose a folder of photos",
                      });
                      if (typeof selected === "string") {
                        setFolderSource(selected);
                      }
                    }}
                  >
                    <FolderOpen size={16} />
                    Browse
                  </button>
                </div>
                <label>Output folder (optional)</label>
                <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <input
                    value={outputDir}
                    readOnly
                    placeholder="Default: 'Frame Sense Corrected' inside the photo folder"
                    style={{ flex: 1, minWidth: 0 }}
                  />
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={async () => {
                      const selected = await openDialog({
                        directory: true,
                        multiple: false,
                        title: "Choose where corrected JPEGs go",
                      });
                      if (typeof selected === "string") {
                        setOutputDir(selected);
                      }
                    }}
                  >
                    <FolderOpen size={16} />
                    Browse
                  </button>
                  {outputDir && (
                    <button type="button" className="icon-button" title="Back to default" onClick={() => setOutputDir("")}>
                      <XCircle size={16} />
                    </button>
                  )}
                </div>
                {toneSuggestBusy && !toneSuggestion && (
                  <div className="field-note">Looking at your photos to suggest a session tone...</div>
                )}
                {toneSuggestion && (
                  <div className="field-note" style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                    <span>
                      Suggested tone for this folder: <strong>{TONES.find((t) => t.id === toneSuggestion.tone)?.label ?? toneSuggestion.tone}</strong>
                      {toneSuggestion.reason ? ` - ${toneSuggestion.reason}` : ""}
                    </span>
                    {toneSuggestion.tone !== look ? (
                      <button type="button" className="secondary-button" onClick={() => setLook(toneSuggestion.tone)}>Apply</button>
                    ) : (
                      <span>(already selected)</span>
                    )}
                    <button type="button" className="icon-button" title="Dismiss suggestion" onClick={() => setToneSuggestion(null)}>
                      <XCircle size={16} />
                    </button>
                  </div>
                )}
                <label>Session tone (optional, Auto is the default): <select value={look} onChange={(e) => setLook(e.target.value)}>
                  {TONES.map((t) => <option key={t.id} value={t.id}>{t.label}{t.id === "auto" ? " (default)" : ""}</option>)}
                </select></label>
                <div className="field-note">
                  {TONES.find((t) => t.id === look)?.hint}
                  <button type="button" className="secondary-button" disabled={previewBusy} onClick={previewRetouch} style={{ marginLeft: 8 }}>
                    {previewBusy ? <RefreshCw className="spin" size={14}/> : null} Preview on a photo
                  </button>
                </div>
                <div className="field-note">Adjust the chosen tone (applies to every photo in this run; 0 = off):</div>
                {([["contrast", "Contrast"], ["brightness", "Brightness (open / darken)"], ["warmth", "Warmth"], ["fade", "Fade"], ["saturation", "Saturation"]] as const).map(([k, label]) => (
                  <label key={k}>{label}: {adj[k].toFixed(2)}
                    <input type="range" min={-1} max={1} step={0.05} value={adj[k]} onChange={(e) => setAdj({ ...adj, [k]: Number(e.target.value) })}/>
                  </label>
                ))}
                {preview?.path && (
                  <button type="button" className="secondary-button" disabled={previewBusy} onClick={() => previewTone(look)}>Update preview with sliders</button>
                )}
                <label>JPEG quality: {quality}<input type="range" min={50} max={100} value={quality} onChange={(e) => setQuality(Number(e.target.value))}/></label>
                <label><input type="checkbox" checked={bw} onChange={(e) => setBw(e.target.checked)}/> Also export a B&W folder</label>
                <label title="Finds faces with a local AI model, then whitens teeth and sharpens skin texture. No smoothing, nothing leaves your PC.">
                  <input type="checkbox" checked={retouch} onChange={(e) => setRetouch(e.target.checked)}/> Face retouch (heal small spots, even skin tone, whiter teeth, brighter eyes)
                </label>
                {retouch && (
                  <div className="field-note">
                    Face retouch runs only on portrait and couple photos where a face is large; hall, venue and crowd photos are skipped. It uses a small AI model on your PC to find facial features (offline; originals never change). Spots, skin tone, teeth and eyes are then adjusted by auto-correction, not AI generation. Moles and facial features are kept. Automatic noise reduction is a classical algorithm (wavelet filter), not AI; it keeps detail and skin texture.
                  </div>
                )}
                {preview && (
                  <div className="import-summary">
                    <strong>Session preview: {preview.name}</strong>
                    <div className="field-note">Optional: pick a session tone to see it on this photo:</div>
                    <div style={{ display: "flex", gap: 6, flexWrap: "wrap", margin: "6px 0" }}>
                      {TONES.map((t) => (
                        <button
                          key={t.id}
                          type="button"
                          className={t.id === look ? "primary-button" : "secondary-button"}
                          disabled={previewBusy}
                          onClick={() => previewTone(t.id)}
                          title={t.hint}
                          style={{ padding: "4px 10px", fontSize: 12 }}
                        >
                          {t.label}
                        </button>
                      ))}
                    </div>
                    <div style={{ display: "flex", gap: 8, alignItems: "flex-start" }}>
                      <figure style={{ margin: 0, flex: 1 }}>
                        <img src={`data:image/jpeg;base64,${preview.before}`} alt="Before" style={{ width: "100%", borderRadius: 6 }} />
                        <figcaption className="field-note">Before (Auto correction only)</figcaption>
                      </figure>
                      <figure style={{ margin: 0, flex: 1 }}>
                        <img src={`data:image/jpeg;base64,${preview.after}`} alt="After" style={{ width: "100%", borderRadius: 6 }} />
                        <figcaption className="field-note">After: {TONES.find((t) => t.id === look)?.label}{retouch ? " + face retouch" : ""}</figcaption>
                      </figure>
                    </div>
                    <button type="button" className="secondary-button" onClick={() => setPreview(null)}>Close preview</button>
                  </div>
                )}
                <button className="primary-button full" disabled={acBusy || !folderSource.trim() || engineStatus?.available === false} onClick={startFolderCorrect}>
                  {acBusy ? <RefreshCw className="spin" size={16}/> : <WandSparkles size={16}/>} Correct this folder
                </button>
                {acProgress && acBusy && (
                  <div className="import-summary">
                    <strong>{acProgress.total > 0 ? `${acProgress.current} / ${acProgress.total}` : "Starting..."}</strong>
                    <span>{acProgress.file}</span>
                    {acProgress.total > 0 && (
                      <div style={{ height: 8, background: "rgba(128,128,128,0.25)", borderRadius: 4, overflow: "hidden" }}>
                        <div style={{ width: `${(acProgress.current / acProgress.total) * 100}%`, height: "100%", background: "currentColor" }} />
                      </div>
                    )}
                    <div style={{ display: "flex", gap: 8 }}>
                      <button
                        type="button"
                        className="secondary-button"
                        onClick={async () => {
                          const next = !acPaused;
                          await invoke("set_processing_control", { command: next ? "pause" : "resume" });
                          setAcPaused(next);
                        }}
                      >
                        {acPaused ? "Resume" : "Pause"}
                      </button>
                      <button
                        type="button"
                        className="secondary-button"
                        onClick={async () => {
                          await invoke("set_processing_control", { command: "stop" });
                          setAcPaused(false);
                        }}
                      >
                        Stop
                      </button>
                      {acPaused && <span className="field-note">Paused</span>}
                    </div>
                  </div>
                )}
                {acReport && (
                  <div className="import-summary">
                    <strong>Auto-correction report</strong>
                    <span>Photos found: {acReport.total}</span>
                    {acReport.stopped && <span>Stopped early</span>}
                    <span>Corrected: {acReport.processed}</span>
                    <span>Already done (skipped): {acReport.already_done}</span>
                    <span>Failed: {acReport.failed.length}</span>
                    {acReport.processed > 0 && <span>Sorted: {acReport.counts.regular} regular, {acReport.counts.couple} couple, {acReport.counts.group} group, {acReport.counts.rejected} rejected</span>}
                    {acReport.counts.rejected > 0 && <small>Rejected photos are copies in the Rejected folder for you to review (clearly blurry, or closed/lowered eyes on the main faces - an automatic check that can miss some blinks or flag a few good photos). Nothing is ever deleted and originals are never touched.</small>}
                    {acReport.total > 0 && <small>{acReport.output_dir}</small>}
                    {acReport.failed.slice(0, 5).map((f) => (
                      <small key={f.file}>{f.file}: {f.error}</small>
                    ))}
                  </div>
                )}
                <p className="field-note">Corrects each photo (exposure, white balance, warm natural skin, straightening) and sorts the copies into Regular / Couple / Group / Rejected subfolders inside the output folder. It does not smooth skin, remove objects or restyle the photo. Originals are never moved, deleted or modified. Re-running skips photos that already have a corrected copy.</p>
              </section>

              <section className="panel">
                <div className="panel-heading"><div><span className="step-tag">FROM A PROJECT</span><h3>Correct a project camera lane</h3><p>For photos already imported into a Frame Sense project. Same correction, per camera lane.</p></div><Gauge/></div>
                <label>Project folder<input value={projectPath} onChange={(e) => setProjectPath(e.target.value)} placeholder="Create a project first, or paste its path"/></label>
                <label>Camera lane<select value={cameraId} onChange={(e) => setCameraId(e.target.value)}><option>Camera 1</option><option>Camera 2</option></select></label>
                <button className="secondary-button full" disabled={acBusy || !projectPath.trim() || engineStatus?.available === false} onClick={startAutoCorrect}>
                  {acBusy ? <RefreshCw className="spin" size={16}/> : <WandSparkles size={16}/>} Correct this camera lane
                </button>
                <p className="field-note">Uses the same output folder and quality as above (default: the project's Export folder). If it reports 0 photos, import the card first on Projects &amp; Import — or just use folder correction above.</p>
              </section>

              <section className="panel">
                <div className="panel-heading"><div><span className="step-tag">ENGINE</span><h3>RAW engine</h3><p>Decodes camera RAW files locally on this computer. No cloud, nothing uploaded.</p></div><Cpu/></div>
                {engineStatus === null && <p className="field-note">Checking engine...</p>}
                {engineStatus?.available && <div className="import-summary"><strong className="status-ok">Connected</strong><span>Version {engineStatus.version}</span><small>{engineStatus.path}</small></div>}
                {engineStatus && !engineStatus.available && <div className="import-summary"><strong>Not available</strong><small>{engineStatus.error}</small></div>}
                <p className="field-note">Honest label: Smart Basic is deterministic histogram correction, not the AI features from the Frame Sense specification. Those remain planned.</p>
              </section>
            </div>
          )}

          {page !== "Dashboard" && page !== "Projects & Import" && page !== "Processing Center" && (
            <section className="planned-panel">
              <div className="planned-icon"><CurrentIcon size={28}/></div>
              <span className="step-tag">ROADMAP MODULE</span>
              <h2>{page}</h2>
              <p>{plannedCopy[page]}</p>
              <div className="planned-status"><span className="status-dot muted"/> Planned – not implemented in this starter</div>
              <button className="secondary-button" onClick={() => setPage("Projects & Import")}><FolderOpen size={16}/> Go to project foundation</button>
            </section>
          )}

          <footer className="content-footer">
            <span><ShieldCheck size={14}/> Local-first by design</span>
            <span><CircleHelp size={14}/> Everything runs on this PC - originals are never changed</span>
            <span><Command size={14}/> v0.6.9</span>
          </footer>
        </div>
      </main>

      <aside className="right-panel">
        <div className="right-head"><span>PROJECT INSPECTOR</span><button className="icon-button"><Settings2 size={16}/></button></div>
        <div className="inspector-block">
          <div className="inspector-label">ACTIVE PROJECT</div>
          <div className="project-mini"><div className="project-mini-icon"><Clapperboard size={18}/></div><div><strong>{projectName || "No project selected"}</strong><small>{projectPath ? "Local project" : "Waiting for project creation"}</small></div></div>
          {projectPath && <div className="path-box">{projectPath}</div>}
        </div>
        <div className="inspector-block">
          <div className="inspector-label">WORKFLOW RULES</div>
          <div className="rule-row"><CheckCircle2 size={15}/><span>One active order</span></div>
          <div className="rule-row"><CheckCircle2 size={15}/><span>Camera lanes stay separate</span></div>
          <div className="rule-row"><CheckCircle2 size={15}/><span>RAW source is immutable</span></div>
          <div className="rule-row"><CheckCircle2 size={15}/><span>JPEG 8-bit final output</span></div>
        </div>
        <div className="inspector-block">
          <div className="inspector-label">ENGINE STATUS</div>
          <div className="engine-row"><span>Photo Engine (RAW + colour)</span><b className={engineStatus?.available ? "status-ok" : "status-planned"}>{engineStatus?.available ? "Ready" : "Not ready"}</b></div>
          <div className="engine-row"><span>Face Retouch (local AI model)</span><b className={engineStatus?.available ? "status-ok" : "status-planned"}>{engineStatus?.available ? "Ready - runs on this PC" : "Not ready"}</b></div>
          <div className="engine-row"><span>Cloud / Internet</span><b className="status-ok">Not needed - all local</b></div>
        </div>
        <div className="inspector-note"><ShieldCheck size={17}/><p>Ready to use: pick a photo folder on the left, choose a session tone, press Start. Corrected copies go to the output folder; original photos are never changed.</p></div>
      </aside>
    </div>
  );
}

export default App;
