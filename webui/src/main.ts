import { UrdfViewport, JointMeta, LinkMeta } from './viewer.ts';
import { InteractionMode, MeasureResult } from './measurement.ts';

declare global {
  interface Window {
    ipc?: {
      postMessage: (msg: string) => void;
    };
  }
}

interface ServerModelPayload {
  type: 'model_update';
  filePath: string;
  fileName: string;
  workingDir: string;
  urdfXml: string | null;
  error: string | null;
  isXacro: boolean;
  xacroArgs: Record<string, string>;
  packages: Record<string, string>;
  diagnostics: string[];
}

const isNativeWindow = new URLSearchParams(window.location.search).get('native') === '1' || Boolean(window.ipc);

// DOM References
const connDot = document.getElementById('conn-dot')!;
const robotTitle = document.getElementById('robot-title')!;
const fileBadge = document.getElementById('file-badge')!;
const statusLive = document.getElementById('toolbar-status')!;
const workspaceGrid = document.getElementById('workspace-grid')!;
const canvasMount = document.getElementById('canvas-mount')!;
const labelsOverlay = document.getElementById('labels-overlay')!;
const emptyBanner = document.getElementById('empty-banner')!;
const errorToast = document.getElementById('error-toast')!;
const hudStats = document.getElementById('hud-stats')!;
const hudHover = document.getElementById('hud-hover')!;
const hudMode = document.getElementById('hud-mode')!;

const btnVisual = document.getElementById('btn-visual') as HTMLButtonElement;
const btnCollision = document.getElementById('btn-collision') as HTMLButtonElement;
const btnInertia = document.getElementById('btn-inertia') as HTMLButtonElement;
const btnTf = document.getElementById('btn-tf') as HTMLButtonElement;
const btnAxes = document.getElementById('btn-axes') as HTMLButtonElement;
const btnModeDrag = document.getElementById('btn-mode-drag') as HTMLButtonElement;
const btnModeDist = document.getElementById('btn-mode-dist') as HTMLButtonElement;
const btnModeAngle = document.getElementById('btn-mode-angle') as HTMLButtonElement;
const btnClearMeasure = document.getElementById('btn-clear-measure') as HTMLButtonElement;
const btnFitCamera = document.getElementById('btn-fit-camera') as HTMLButtonElement;
const btnPinTop = document.getElementById('btn-pin-top') as HTMLButtonElement;
const btnOpenBrowser = document.getElementById('btn-open-browser') as HTMLButtonElement;
const btnTheme = document.getElementById('btn-theme') as HTMLButtonElement;
const btnToggleSidebar = document.getElementById('btn-toggle-sidebar') as HTMLButtonElement;

const jointCountEl = document.getElementById('joint-count')!;
const linkCountEl = document.getElementById('link-count')!;
const jointsListEl = document.getElementById('joints-list')!;
const linksListEl = document.getElementById('links-list')!;
const linkFilterInput = document.getElementById('link-filter') as HTMLInputElement;
const btnZeroJoints = document.getElementById('btn-zero-joints') as HTMLButtonElement;
const btnRandomJoints = document.getElementById('btn-random-joints') as HTMLButtonElement;
const btnUnitToggle = document.getElementById('btn-unit-toggle') as HTMLButtonElement;
const tfSizeSlider = document.getElementById('tf-size-slider') as HTMLInputElement;
const tfSizeVal = document.getElementById('tf-size-val')!;
const btnShowAllLinks = document.getElementById('btn-show-all-links') as HTMLButtonElement;
const btnHideAllLinks = document.getElementById('btn-hide-all-links') as HTMLButtonElement;

const measureDetails = document.getElementById('measure-details') as HTMLDetailsElement;
const measureHint = document.getElementById('measure-hint')!;
const measureReadout = document.getElementById('measure-readout')!;
const xacroArgsSection = document.getElementById('xacro-args-section')!;
const xacroArgsList = document.getElementById('xacro-args-list')!;
const packagesListEl = document.getElementById('packages-list')!;
const meshWarningsBox = document.getElementById('mesh-warnings-box')!;
const warningsListEl = document.getElementById('warnings-list')!;

let useDegrees = false;
let currentJoints: JointMeta[] = [];
let currentLinks: LinkMeta[] = [];
let currentDiagnostics: string[] = [];
let currentFilePath = '';
let ws: WebSocket | null = null;
const jointDomMap = new Map<string, { slider: HTMLInputElement; num: HTMLInputElement; isAngular: boolean }>();

function syncSliderVars(slider: HTMLInputElement) {
  slider.style.setProperty('--attr-min', slider.min || '0');
  slider.style.setProperty('--attr-max', slider.max || '100');
  slider.style.setProperty('--control-value', slider.value);
}

syncSliderVars(tfSizeSlider);

const viewport = new UrdfViewport(canvasMount, labelsOverlay, {
  onRobotLoaded({ robotName, joints, links, meshWarnings }) {
    emptyBanner.hidden = true;
    const name = robotName || 'URDF Robot';
    robotTitle.textContent = name;
    document.title = fileBadge.textContent
      ? `URDF Visualizer — ${name} (${fileBadge.textContent})`
      : `URDF Visualizer — ${name}`;
    hudStats.textContent = `Links: ${links.length} · Movable Joints: ${joints.length}`;
    currentJoints = joints;
    currentLinks = links;
    renderJointsPanel(joints);
    renderLinksPanel(links);
    renderWarnings([...currentDiagnostics, ...meshWarnings]);
  },
  onJointDrag(jointName, value) {
    const dom = jointDomMap.get(jointName);
    if (!dom) return;
    dom.slider.value = String(value);
    syncSliderVars(dom.slider);
    const displayVal = dom.isAngular && useDegrees ? (value * 180) / Math.PI : value;
    dom.num.value = displayVal.toFixed(dom.isAngular && useDegrees ? 1 : 3);
    dom.slider.setAttribute(
      'aria-valuetext',
      `${displayVal.toFixed(2)} ${dom.isAngular ? (useDegrees ? 'deg' : 'rad') : 'm'}`
    );
  },
  onHoverPoint(pt, linkName) {
    if (!pt) {
      hudHover.textContent = 'ROS 坐标 (Z-Up): —';
      return;
    }
    const suffix = linkName ? ` [${linkName}]` : '';
    hudHover.textContent = `XYZ: (${pt.x.toFixed(3)}, ${pt.y.toFixed(3)}, ${pt.z.toFixed(3)}) m${suffix}`;
  },
  onMeasureUpdate(res) {
    measureDetails.open = true;
    renderMeasureResult(res);
  },
  onSelectLink(linkName) {
    for (const row of linksListEl.querySelectorAll('.link-row')) {
      row.classList.toggle('selected-link', (row as HTMLElement).dataset.link === linkName);
    }
  }
});

// Accessible Toolbar focusgroup fallback (Modern Web Guidance `toolbar`)
const toolbarEl = document.getElementById('main-toolbar')!;
if (!('focusGroup' in HTMLElement.prototype)) {
  const getToolbarButtons = () =>
    Array.from(toolbarEl.querySelectorAll<HTMLButtonElement>('button:not([hidden])'));

  const initRovingTabindex = () => {
    const items = getToolbarButtons();
    items.forEach((b, i) => (b.tabIndex = i === 0 ? 0 : -1));
  };
  initRovingTabindex();

  toolbarEl.addEventListener('focusin', ({ target }) => {
    const items = getToolbarButtons();
    if (target instanceof HTMLButtonElement && items.includes(target)) {
      items.forEach(b => (b.tabIndex = b === target ? 0 : -1));
    }
  });

  toolbarEl.addEventListener('keydown', e => {
    const items = getToolbarButtons();
    const idx = items.indexOf(document.activeElement as HTMLButtonElement);
    if (idx < 0) return;
    const rtl = getComputedStyle(toolbarEl).direction === 'rtl';
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + (rtl ? -1 : 1) + items.length) % items.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx + (rtl ? 1 : -1) + items.length) % items.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = items.length - 1;

    if (nextIdx >= 0) {
      e.preventDefault();
      items[nextIdx].focus();
    }
  });
}

// Native Window Controls (wry IPC)
if (isNativeWindow) {
  btnPinTop.hidden = false;
  btnOpenBrowser.hidden = false;

  document.getElementById('titlebar')!.addEventListener('mousedown', e => {
    const target = e.target as HTMLElement;
    if (e.button === 0 && !target.closest('button, input, select, a, label')) {
      window.ipc?.postMessage(JSON.stringify({ cmd: 'drag_window' }));
    }
  });

  btnPinTop.addEventListener('click', () => {
    const next = btnPinTop.getAttribute('aria-pressed') !== 'true';
    btnPinTop.setAttribute('aria-pressed', String(next));
    window.ipc?.postMessage(JSON.stringify({ cmd: 'set_always_on_top', value: next }));
    statusLive.textContent = next ? '已开启窗口置顶' : '已关闭窗口置顶';
  });

  btnOpenBrowser.addEventListener('click', () => {
    const cleanUrl = new URL(window.location.href);
    cleanUrl.searchParams.delete('native');
    window.ipc?.postMessage(JSON.stringify({ cmd: 'open_browser', url: cleanUrl.toString() }));
    statusLive.textContent = '已在默认浏览器打开';
  });
}

// Display Toggles
function bindToggle(btn: HTMLButtonElement, onToggle: (active: boolean) => void) {
  btn.addEventListener('click', () => {
    const next = btn.getAttribute('aria-pressed') !== 'true';
    btn.setAttribute('aria-pressed', String(next));
    onToggle(next);
  });
}

bindToggle(btnVisual, active => {
  viewport.showVisual = active;
  viewport.applyVisibility();
});
bindToggle(btnCollision, active => {
  viewport.showCollision = active;
  viewport.applyVisibility();
});
bindToggle(btnInertia, active => {
  viewport.showInertia = active;
  viewport.applyVisibility();
});
bindToggle(btnTf, active => {
  viewport.showTf = active;
  viewport.applyVisibility();
});
bindToggle(btnAxes, active => {
  viewport.showJointAxes = active;
  viewport.applyVisibility();
});

// Mutually Exclusive Interaction Mode Group
function setInteractionMode(mode: InteractionMode) {
  viewport.interactionMode = mode;
  btnModeDrag.setAttribute('aria-pressed', String(mode === 'drag'));
  btnModeDist.setAttribute('aria-pressed', String(mode === 'distance'));
  btnModeAngle.setAttribute('aria-pressed', String(mode === 'angle'));

  if (mode === 'drag') {
    hudMode.textContent = '模式: 拖拽关节';
    measureHint.textContent = '点击上方「测距」或「测角」后在模型表面取点。';
  } else if (mode === 'distance') {
    hudMode.textContent = '模式: 两点测距 (点击模型表面两点)';
    measureDetails.open = true;
    measureHint.textContent = '在机器人网格表面依次点击 P1、P2 计算三维距离与轴向分量。';
  } else {
    hudMode.textContent = '模式: 三点测角 (依次点击 P1 → 顶点 P2 → P3)';
    measureDetails.open = true;
    measureHint.textContent = '依次点击起点 P1、角顶点 P2、终点 P3 计算三维空间夹角。';
  }
}

btnModeDrag.addEventListener('click', () => setInteractionMode('drag'));
btnModeDist.addEventListener('click', () => setInteractionMode('distance'));
btnModeAngle.addEventListener('click', () => setInteractionMode('angle'));

btnClearMeasure.addEventListener('click', () => {
  viewport.clearMeasurements();
  measureReadout.replaceChildren();
  statusLive.textContent = '已清除测量标记';
});

btnFitCamera.addEventListener('click', () => {
  viewport.fitCameraToRobot();
  statusLive.textContent = '已重置相机视角';
});

window.addEventListener('keydown', e => {
  if (e.target instanceof HTMLInputElement) return;
  if (e.key === 'f' || e.key === 'F') viewport.fitCameraToRobot();
  if (e.key === 'Escape' && viewport.interactionMode !== 'drag') setInteractionMode('drag');
});

// Two-State System-Aware Theme Toggle (Modern Web Guidance `dark-mode-toggle`)
const metaScheme = document.querySelector('meta[name="color-scheme"]') as HTMLMetaElement;
const darkMql = window.matchMedia('(prefers-color-scheme: dark)');

btnTheme.addEventListener('click', () => {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem('color-scheme');
  } catch {}
  const systemScheme = darkMql.matches ? 'dark' : 'light';
  const currentRendered = stored === 'dark' || stored === 'light' ? stored : systemScheme;
  const target = currentRendered === 'dark' ? 'light' : 'dark';

  if (target === systemScheme) {
    try {
      localStorage.removeItem('color-scheme');
    } catch {}
    metaScheme.content = 'light dark';
  } else {
    try {
      localStorage.setItem('color-scheme', target);
    } catch {}
    metaScheme.content = target;
  }
  viewport.rebuildThemeScene(target === 'dark');
});

darkMql.addEventListener('change', () => {
  viewport.rebuildThemeScene(viewport.isDarkActive());
});

window.addEventListener('storage', e => {
  if (e.key === 'color-scheme') {
    metaScheme.content = e.newValue ?? 'light dark';
    viewport.rebuildThemeScene(viewport.isDarkActive());
  }
});

bindToggle(btnToggleSidebar, open => {
  workspaceGrid.classList.toggle('sidebar-collapsed', !open);
});

tfSizeSlider.addEventListener('input', () => {
  syncSliderVars(tfSizeSlider);
  const val = parseFloat(tfSizeSlider.value);
  tfSizeVal.textContent = `${val.toFixed(2)}m`;
  viewport.setTfScale(val);
});

btnZeroJoints.addEventListener('click', () => viewport.resetAllJoints());
btnRandomJoints.addEventListener('click', () => viewport.randomizeJoints());

bindToggle(btnUnitToggle, deg => {
  useDegrees = deg;
  btnUnitToggle.textContent = `单位: ${deg ? 'deg' : 'rad'}`;
  renderJointsPanel(currentJoints);
});

// Render Joints Inspector
function renderJointsPanel(joints: JointMeta[]) {
  jointCountEl.textContent = String(joints.length);
  jointDomMap.clear();
  jointsListEl.replaceChildren();

  if (joints.length === 0) {
    const p = document.createElement('p');
    p.className = 'muted-note';
    p.textContent = '当前模型无活动关节 (仅 fixed 关节)';
    jointsListEl.appendChild(p);
    return;
  }

  for (const j of joints) {
    const isAngular = j.jointType !== 'prismatic';
    const card = document.createElement('div');
    card.className = 'joint-card';

    const header = document.createElement('div');
    header.className = 'joint-header';

    const nameBtn = document.createElement('button');
    nameBtn.type = 'button';
    nameBtn.className = 'joint-name-btn';
    nameBtn.textContent = j.name;
    nameBtn.title = '点击在 Zed 编辑器中定位此关节定义';
    nameBtn.addEventListener('click', () => jumpToSymbol('joint', j.name));

    const badge = document.createElement('span');
    badge.className = 'joint-badge';
    badge.textContent = j.jointType;

    header.append(nameBtn, badge);

    const controls = document.createElement('div');
    controls.className = 'joint-controls';

    const sliderWrap = document.createElement('div');
    sliderWrap.className = 'joint-slider-wrap';

    const sliderId = `joint-slider-${j.name}`;
    const slider = document.createElement('input');
    slider.type = 'range';
    slider.id = sliderId;
    slider.setAttribute('aria-label', `${j.name} 关节位置`);
    slider.min = String(j.lower);
    slider.max = String(j.upper);
    slider.step = String(Math.max(0.001, (j.upper - j.lower) / 400));
    slider.value = String(j.value);
    syncSliderVars(slider);

    const formatLimit = (radOrM: number) => {
      if (!isAngular) return `${radOrM.toFixed(3)} m`;
      if (useDegrees) return `${((radOrM * 180) / Math.PI).toFixed(1)}°`;
      return `${radOrM.toFixed(2)} rad`;
    };

    const num = document.createElement('input');
    num.type = 'number';
    num.className = 'joint-num-input';
    num.setAttribute('aria-label', `${j.name} 数值输入`);
    num.step = isAngular && useDegrees ? '1' : '0.01';
    const disp = isAngular && useDegrees ? (j.value * 180) / Math.PI : j.value;
    num.value = disp.toFixed(isAngular && useDegrees ? 1 : 3);

    const applyJointVal = (val: number) => {
      const clamped = Math.max(j.lower, Math.min(j.upper, val));
      j.value = clamped;
      slider.value = String(clamped);
      syncSliderVars(slider);
      viewport.setJointValue(j.name, clamped);
      const shown = isAngular && useDegrees ? (clamped * 180) / Math.PI : clamped;
      num.value = shown.toFixed(isAngular && useDegrees ? 1 : 3);
    };

    const limitsRow = document.createElement('div');
    limitsRow.className = 'joint-limits';

    const minBtn = document.createElement('button');
    minBtn.type = 'button';
    minBtn.className = 'joint-limit-btn';
    minBtn.textContent = j.jointType === 'continuous' ? `${formatLimit(j.lower)} (∞)` : formatLimit(j.lower);
    minBtn.title = '点击跳转到下限';
    minBtn.addEventListener('click', () => applyJointVal(j.lower));

    const maxBtn = document.createElement('button');
    maxBtn.type = 'button';
    maxBtn.className = 'joint-limit-btn';
    maxBtn.textContent = j.jointType === 'continuous' ? `${formatLimit(j.upper)} (∞)` : formatLimit(j.upper);
    maxBtn.title = '点击跳转到上限';
    maxBtn.addEventListener('click', () => applyJointVal(j.upper));

    if (j.lower < -1e-4 && j.upper > 1e-4) {
      const zeroBtn = document.createElement('button');
      zeroBtn.type = 'button';
      zeroBtn.className = 'joint-limit-btn';
      zeroBtn.textContent = '0';
      zeroBtn.title = '点击归零';
      zeroBtn.addEventListener('click', () => applyJointVal(0));
      limitsRow.append(minBtn, zeroBtn, maxBtn);
    } else {
      limitsRow.append(minBtn, maxBtn);
    }

    sliderWrap.append(slider, limitsRow);

    slider.addEventListener('input', () => {
      syncSliderVars(slider);
      const radOrM = parseFloat(slider.value);
      j.value = radOrM;
      viewport.setJointValue(j.name, radOrM);
      const shown = isAngular && useDegrees ? (radOrM * 180) / Math.PI : radOrM;
      num.value = shown.toFixed(isAngular && useDegrees ? 1 : 3);
    });

    num.addEventListener('change', () => {
      const parsed = parseFloat(num.value);
      if (!Number.isFinite(parsed)) return;
      const radOrM = isAngular && useDegrees ? (parsed * Math.PI) / 180 : parsed;
      applyJointVal(radOrM);
    });

    controls.append(sliderWrap, num);
    card.append(header, controls);
    jointsListEl.appendChild(card);
    jointDomMap.set(j.name, { slider, num, isAngular });
  }
}

// Render Links Inspector
function renderLinksPanel(links: LinkMeta[]) {
  linkCountEl.textContent = String(links.length);
  const query = linkFilterInput.value.trim().toLowerCase();
  linksListEl.replaceChildren();

  for (const link of links) {
    if (query && !link.name.toLowerCase().includes(query)) continue;
    const li = document.createElement('li');
    li.className = 'link-row';
    li.dataset.link = link.name;

    const label = document.createElement('label');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = link.visible;
    cb.addEventListener('change', () => {
      link.visible = cb.checked;
      viewport.setLinkVisible(link.name, cb.checked);
    });

    const span = document.createElement('span');
    span.textContent = link.mass > 0 ? `${link.name} (${link.mass.toFixed(2)}kg)` : link.name;
    label.append(cb, span);

    const jumpBtn = document.createElement('button');
    jumpBtn.type = 'button';
    jumpBtn.className = 'link-jump-btn';
    jumpBtn.textContent = '定位';
    jumpBtn.ariaLabel = `在 Zed 中定位连杆 ${link.name}`;
    jumpBtn.addEventListener('click', () => jumpToSymbol('link', link.name));

    li.append(label, jumpBtn);
    linksListEl.appendChild(li);
  }
}

linkFilterInput.addEventListener('input', () => renderLinksPanel(currentLinks));

btnShowAllLinks.addEventListener('click', () => {
  currentLinks.forEach(l => (l.visible = true));
  viewport.setAllLinksVisible(true);
  renderLinksPanel(currentLinks);
});

btnHideAllLinks.addEventListener('click', () => {
  currentLinks.forEach(l => (l.visible = false));
  viewport.setAllLinksVisible(false);
  renderLinksPanel(currentLinks);
});

function renderMeasureResult(res: MeasureResult) {
  measureReadout.replaceChildren();
  const addRow = (k: string, v: string) => {
    const dt = document.createElement('dt');
    dt.textContent = k;
    const dd = document.createElement('dd');
    dd.textContent = v;
    measureReadout.append(dt, dd);
  };

  res.points.forEach((p, idx) => {
    addRow(`P${idx + 1}`, `(${p.x.toFixed(4)}, ${p.y.toFixed(4)}, ${p.z.toFixed(4)}) m`);
  });

  if (res.mode === 'distance' && res.distance !== undefined && res.delta) {
    addRow('直线距离 d', `${res.distance.toFixed(4)} m (${(res.distance * 1000).toFixed(2)} mm)`);
    addRow('ΔX / ΔY / ΔZ', `${res.delta.x.toFixed(4)} / ${res.delta.y.toFixed(4)} / ${res.delta.z.toFixed(4)} m`);
  }
  if (res.mode === 'angle' && res.angleDeg !== undefined && res.angleRad !== undefined) {
    addRow('空间夹角 θ', `${res.angleDeg.toFixed(2)}° (${res.angleRad.toFixed(4)} rad)`);
  }
}

function renderPackagesAndXacro(payload: ServerModelPayload) {
  packagesListEl.replaceChildren();
  const entries = Object.entries(payload.packages || {});
  if (entries.length === 0) {
    const li = document.createElement('li');
    li.className = 'muted-note';
    li.textContent = '未检测到 ROS package.xml 映射 (可在 .zed/settings.json 配置)';
    packagesListEl.appendChild(li);
  } else {
    for (const [pkg, dir] of entries) {
      const li = document.createElement('li');
      li.textContent = `${pkg} → ${dir}`;
      packagesListEl.appendChild(li);
    }
  }

  const argEntries = Object.entries(payload.xacroArgs || {});
  if (payload.isXacro && argEntries.length > 0) {
    xacroArgsSection.hidden = false;
    xacroArgsList.replaceChildren();
    for (const [argName, argVal] of argEntries) {
      const row = document.createElement('div');
      row.className = 'xacro-arg-row';
      const lbl = document.createElement('label');
      lbl.textContent = argName;
      const inp = document.createElement('input');
      inp.type = 'text';
      inp.className = 'xacro-arg-input';
      inp.value = argVal;
      lbl.htmlFor = `xacro-arg-${argName}`;
      inp.id = `xacro-arg-${argName}`;
      inp.addEventListener('change', () => {
        ws?.send(JSON.stringify({ type: 'set_xacro_arg', name: argName, value: inp.value }));
      });
      row.append(lbl, inp);
      xacroArgsList.appendChild(row);
    }
  } else {
    xacroArgsSection.hidden = true;
  }
}

function renderWarnings(warnings: string[]) {
  warningsListEl.replaceChildren();
  if (warnings.length === 0) {
    meshWarningsBox.hidden = true;
    return;
  }
  meshWarningsBox.hidden = false;
  for (const w of warnings) {
    const li = document.createElement('li');
    li.textContent = w;
    warningsListEl.appendChild(li);
  }
}

function jumpToSymbol(kind: 'link' | 'joint', name: string) {
  if (!currentFilePath) return;
  ws?.send(JSON.stringify({ type: 'jump_to_symbol', filePath: currentFilePath, kind, name }));
}

function handleServerPayload(payload: ServerModelPayload) {
  currentFilePath = payload.filePath;
  fileBadge.textContent = payload.fileName || '未命名';
  fileBadge.title = payload.filePath || '';
  currentDiagnostics = payload.diagnostics || [];
  renderPackagesAndXacro(payload);

  if (payload.error) {
    errorToast.hidden = false;
    errorToast.textContent = payload.error;
  } else {
    errorToast.hidden = true;
  }

  if (payload.urdfXml) {
    try {
      viewport.loadUrdfXml(payload.urdfXml, payload.workingDir, payload.filePath || payload.fileName);
    } catch (err) {
      errorToast.hidden = false;
      errorToast.textContent = err instanceof Error ? err.message : String(err);
    }
  }
}

// WebSocket Live Connection to zed-urdf-server
function connectWebSocket() {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const socket = new WebSocket(`${proto}//${window.location.host}/ws`);
  ws = socket;

  socket.addEventListener('open', () => {
    connDot.classList.add('connected');
    if (fileBadge.textContent === '等待连接…') {
      fileBadge.textContent = '已连接 LSP';
    }
  });

  socket.addEventListener('message', ev => {
    try {
      const msg = JSON.parse(String(ev.data));
      if (msg.type === 'model_update') {
        handleServerPayload(msg as ServerModelPayload);
      } else if (msg.type === 'highlight_symbol' && msg.kind === 'link') {
        for (const row of linksListEl.querySelectorAll('.link-row')) {
          row.classList.toggle('selected-link', (row as HTMLElement).dataset.link === msg.name);
        }
      }
    } catch (e) {
      console.error('WS message error:', e);
    }
  });

  socket.addEventListener('close', () => {
    connDot.classList.remove('connected');
    setTimeout(connectWebSocket, 1200);
  });
}

connectWebSocket();

// Support drag-and-drop standalone .urdf file directly into window
window.addEventListener('dragover', e => e.preventDefault());
window.addEventListener('drop', async e => {
  e.preventDefault();
  const file = e.dataTransfer?.files?.[0];
  if (!file) return;
  const text = await file.text();
  emptyBanner.hidden = true;
  errorToast.hidden = true;
  fileBadge.textContent = file.name;
  try {
    viewport.loadUrdfXml(text, '', file.name);
  } catch (err) {
    errorToast.hidden = false;
    errorToast.textContent = err instanceof Error ? err.message : String(err);
  }
});
