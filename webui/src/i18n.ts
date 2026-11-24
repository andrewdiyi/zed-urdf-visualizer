export type UiLang = 'en' | 'zh';

const STORAGE_KEY = 'urdf-visualizer-lang';

const DICT = {
  en: {
    waitingConnect: 'Connecting…',
    connectedLsp: 'LSP Connected',
    unnamed: 'Untitled',
    toolbarAria: '3D Viewport Toolbar',
    groupLayers: 'Model Layers',
    groupInteract: 'Interaction & Measurement',
    groupWindow: 'View & Window',
    tipVisual: 'Toggle Visual Meshes',
    tipCollision: 'Toggle Collision Geometry',
    tipInertia: 'Toggle Inertia Principal Box & CoM',
    tipTf: 'Toggle Link TF Coordinate Frames',
    tipAxes: 'Toggle Joint Motion Axes',
    dragJoints: 'Drag Joints',
    tipDragJoints: 'Drag robot links directly in 3D to move joints',
    measureDist: 'Distance',
    tipMeasureDist: 'Measure 3D distance between two surface points',
    measureAngle: 'Angle',
    tipMeasureAngle: 'Measure 3D angle across three points (P1 → Vertex P2 → P3)',
    tipClearMeasure: 'Clear measurements',
    tipFitCamera: 'Reset camera view (F)',
    pin: 'Pin',
    tipPin: 'Keep floating window always on top',
    tipOpenBrowser: 'Open in default browser',
    tipTheme: 'Toggle light/dark theme',
    tipLang: 'Switch UI language (English / 简体中文)',
    tipSidebar: 'Toggle inspector sidebar',
    emptyTitle: 'Waiting for URDF / Xacro Model',
    emptyDesc: 'Open any .urdf or .xacro file in Zed to preview live, or drag and drop a file into this window.',
    hudHoverIdle: 'ROS XYZ (Z-Up): —',
    hudStats: 'Links: {links} · Movable Joints: {joints}',
    modeDrag: 'Mode: Drag Joints',
    modeDist: 'Mode: Distance (pick 2 points on model)',
    modeAngle: 'Mode: Angle (pick P1 → Vertex P2 → P3)',
    hintDrag: 'Select "Distance" or "Angle" above, then pick points on the model surface.',
    hintDist: 'Click P1 and P2 on the robot surface to compute 3D distance and axis deltas.',
    hintAngle: 'Click P1, vertex P2, and P3 to compute the 3D angle.',
    secJoints: 'Joints',
    zeroAll: 'Zero All',
    randomPose: 'Random Pose',
    unitLabel: 'Unit: {unit}',
    axisSize: 'Axis Size',
    noMovableJoints: 'No movable joints (fixed joints only)',
    secLinks: 'Links & Visibility',
    searchLinks: 'Filter links…',
    showAll: 'Show All',
    hideAll: 'Hide All',
    locate: 'Locate',
    locateJointTip: 'Jump to this joint definition in Zed',
    locateLinkAria: 'Jump to link {name} in Zed',
    snapMinTip: 'Snap to lower limit',
    snapZeroTip: 'Snap to zero',
    snapMaxTip: 'Snap to upper limit',
    secMeasure: 'Measurements',
    distLinear: 'Distance d',
    angle3d: 'Angle θ',
    secPackages: 'ROS Packages & Xacro',
    xacroArgsTitle: 'Xacro Arguments (xacro:arg)',
    pkgMapTitle: 'Resolved package:// Mappings',
    noPackages: 'No ROS package.xml found (configure in .zed/settings.json)',
    warningsTitle: 'Mesh & Diagnostic Warnings',
    statusPinOn: 'Always-on-top enabled',
    statusPinOff: 'Always-on-top disabled',
    statusBrowser: 'Opened in default browser',
    statusCleared: 'Measurements cleared',
    statusCameraReset: 'Camera reset',
    langBtnText: '中文'
  },
  zh: {
    waitingConnect: '等待连接…',
    connectedLsp: '已连接 LSP',
    unnamed: '未命名',
    toolbarAria: '三维视图与工具栏',
    groupLayers: '模型图层显示',
    groupInteract: '交互与测量模式',
    groupWindow: '视角与窗口操作',
    tipVisual: '显示外观网格 (Visual)',
    tipCollision: '显示碰撞体 (Collision)',
    tipInertia: '显示惯性主轴包络与质心 (Inertia)',
    tipTf: '显示连杆 TF 坐标系 (TF Frames)',
    tipAxes: '显示关节运动轴向箭头 (Joint Axes)',
    dragJoints: '拖拽关节',
    tipDragJoints: '鼠标直接拖拽模型关节',
    measureDist: '测距',
    tipMeasureDist: '两点距离测量 (点击模型表面两点)',
    measureAngle: '测角',
    tipMeasureAngle: '三点夹角测量 (依次点击起点、顶点、终点)',
    tipClearMeasure: '清除测量标记',
    tipFitCamera: '自适应重置相机视角 (F)',
    pin: '置顶',
    tipPin: '悬浮窗置顶贴靠 Zed',
    tipOpenBrowser: '在默认浏览器中打开',
    tipTheme: '切换深浅色外观',
    tipLang: '切换界面语言 (English / 简体中文)',
    tipSidebar: '展开/收起侧边检查器',
    emptyTitle: '等待打开 URDF / Xacro 模型',
    emptyDesc: '在 Zed 中打开任意 .urdf 或 .xacro 文件即可实时同步预览，也可以直接将文件拖入此窗口。',
    hudHoverIdle: 'ROS 坐标 (Z-Up): —',
    hudStats: 'Links: {links} · 可动关节: {joints}',
    modeDrag: '模式: 拖拽关节',
    modeDist: '模式: 两点测距 (点击模型表面两点)',
    modeAngle: '模式: 三点测角 (依次点击 P1 → 顶点 P2 → P3)',
    hintDrag: '点击上方「测距」或「测角」后在模型表面取点。',
    hintDist: '在机器人网格表面依次点击 P1、P2 计算三维距离与轴向分量。',
    hintAngle: '依次点击起点 P1、角顶点 P2、终点 P3 计算三维空间夹角。',
    secJoints: '关节运动控制',
    zeroAll: '全部归零',
    randomPose: '随机姿态',
    unitLabel: '单位: {unit}',
    axisSize: '坐标轴尺寸',
    noMovableJoints: '当前模型无活动关节 (仅 fixed 关节)',
    secLinks: '连杆结构与显隐',
    searchLinks: '搜索 Link 名称…',
    showAll: '全部显示',
    hideAll: '全部隐藏',
    locate: '定位',
    locateJointTip: '点击在 Zed 编辑器中定位此关节定义',
    locateLinkAria: '在 Zed 中定位连杆 {name}',
    snapMinTip: '点击跳转到下限',
    snapZeroTip: '点击归零',
    snapMaxTip: '点击跳转到上限',
    secMeasure: '测量工具与选点数据',
    distLinear: '直线距离 d',
    angle3d: '空间夹角 θ',
    secPackages: 'ROS 包寻址与 Xacro 状态',
    xacroArgsTitle: 'Xacro 参数覆盖 (xacro:arg)',
    pkgMapTitle: '已解析 package:// 映射',
    noPackages: '未检测到 ROS package.xml 映射 (可在 .zed/settings.json 配置)',
    warningsTitle: '网格/诊断告警',
    statusPinOn: '已开启窗口置顶',
    statusPinOff: '已关闭窗口置顶',
    statusBrowser: '已在默认浏览器打开',
    statusCleared: '已清除测量标记',
    statusCameraReset: '已重置相机视角',
    langBtnText: 'EN'
  }
} as const;

export type I18nKey = keyof typeof DICT.en;

let currentLang: UiLang = loadInitialLang();

function loadInitialLang(): UiLang {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'en' || stored === 'zh') return stored;
  } catch {}
  return 'en';
}

export function getLang(): UiLang {
  return currentLang;
}

export function hasStoredLang(): boolean {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return v === 'en' || v === 'zh';
  } catch {
    return false;
  }
}

export function setLang(lang: UiLang, persist = true) {
  currentLang = lang;
  document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
  if (persist) {
    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch {}
  }
}

export function t(key: I18nKey, vars?: Record<string, string | number>): string {
  let text: string = DICT[currentLang][key] ?? DICT.en[key];
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      text = text.replace(`{${k}}`, String(v));
    }
  }
  return text;
}

export function applyStaticI18n() {
  document.documentElement.lang = currentLang === 'zh' ? 'zh-CN' : 'en';
  for (const el of document.querySelectorAll<HTMLElement>('[data-i18n]')) {
    const k = el.dataset.i18n as I18nKey;
    if (k in DICT.en) el.textContent = t(k);
  }
  for (const el of document.querySelectorAll<HTMLElement>('[data-i18n-title]')) {
    const k = el.dataset.i18nTitle as I18nKey;
    if (k in DICT.en) el.title = t(k);
  }
  for (const el of document.querySelectorAll<HTMLInputElement>('[data-i18n-placeholder]')) {
    const k = el.dataset.i18nPlaceholder as I18nKey;
    if (k in DICT.en) el.placeholder = t(k);
  }
  for (const el of document.querySelectorAll<HTMLElement>('[data-i18n-aria]')) {
    const k = el.dataset.i18nAria as I18nKey;
    if (k in DICT.en) el.setAttribute('aria-label', t(k));
  }
}
