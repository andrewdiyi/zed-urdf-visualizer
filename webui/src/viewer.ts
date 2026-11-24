import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js';
import { ColladaLoader } from 'three/examples/jsm/loaders/ColladaLoader.js';
import { OBJLoader } from 'three/examples/jsm/loaders/OBJLoader.js';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import URDFLoader, { URDFRobot, URDFJoint, URDFLink } from 'urdf-loader';
import { createInertiaVisual } from './inertia.ts';
import { InteractionMode, MeasureResult, MeasurementController } from './measurement.ts';

const OVERLAY_LAYER = 1;

export interface JointMeta {
  name: string;
  jointType: string;
  lower: number;
  upper: number;
  value: number;
}

export interface LinkMeta {
  name: string;
  mass: number;
  visible: boolean;
}

export interface ViewerCallbacks {
  onRobotLoaded: (info: {
    robotName: string;
    joints: JointMeta[];
    links: LinkMeta[];
    meshWarnings: string[];
  }) => void;
  onJointDrag: (jointName: string, value: number) => void;
  onHoverPoint: (pt: THREE.Vector3 | null, linkName: string | null) => void;
  onMeasureUpdate: (res: MeasureResult) => void;
  onSelectLink: (linkName: string | null) => void;
}

export class UrdfViewport {
  private container: HTMLElement;
  private scene: THREE.Scene;
  private camera: THREE.PerspectiveCamera;
  private renderer: THREE.WebGLRenderer;
  private orbit: OrbitControls;
  private measure: MeasurementController;
  private callbacks: ViewerCallbacks;

  private robot: URDFRobot | null = null;
  private currentModelKey = '';
  private savedJointValues = new Map<string, number>();
  private hiddenLinks = new Set<string>();
  private tfHelpers: THREE.Object3D[] = [];
  private axisHelpers: THREE.Object3D[] = [];
  private inertiaHelpers: THREE.Object3D[] = [];

  public showVisual = true;
  public showCollision = false;
  public showInertia = false;
  public showTf = false;
  public showJointAxes = false;
  public tfScale = 0.15;
  public interactionMode: InteractionMode = 'drag';

  // Drag & raycast state
  private raycaster = new THREE.Raycaster();
  private pointer = new THREE.Vector2();
  private draggingJoint: URDFJoint | null = null;
  private dragHitDistance = 1;
  private initialGrabPoint = new THREE.Vector3();
  private prevRay = new THREE.Ray();
  private pointerDownPos = new THREE.Vector2();

  constructor(
    mountEl: HTMLElement,
    labelsEl: HTMLElement,
    callbacks: ViewerCallbacks
  ) {
    this.container = mountEl;
    this.callbacks = callbacks;

    THREE.Object3D.DEFAULT_UP.set(0, 0, 1);

    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(48, 1, 0.005, 200);
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(1.6, -1.6, 1.2);
    this.camera.layers.enable(OVERLAY_LAYER);

    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    mountEl.appendChild(this.renderer.domElement);

    this.orbit = new OrbitControls(this.camera, this.renderer.domElement);
    this.orbit.enableDamping = true;
    this.orbit.dampingFactor = 0.08;
    this.orbit.target.set(0, 0, 0.25);

    this.setupLightsAndGrid();
    this.measure = new MeasurementController(this.scene, labelsEl, OVERLAY_LAYER);
    this.raycaster.layers.set(0);

    this.bindPointerEvents();
    this.handleResize();
    const ro = new ResizeObserver(() => this.handleResize());
    ro.observe(mountEl);

    this.animate();
  }

  private setupLightsAndGrid() {
    const hemi = new THREE.HemisphereLight(0xffffff, 0x444c5e, 1.15);
    hemi.position.set(0, 0, 5);
    this.scene.add(hemi);

    const dir1 = new THREE.DirectionalLight(0xffffff, 1.35);
    dir1.position.set(3, -4, 5);
    dir1.castShadow = true;
    this.scene.add(dir1);

    const dir2 = new THREE.DirectionalLight(0xdbeafe, 0.65);
    dir2.position.set(-4, 3, 2.5);
    this.scene.add(dir2);

    this.rebuildThemeScene(this.isDarkActive());
  }

  public isDarkActive(): boolean {
    const meta = document.querySelector('meta[name="color-scheme"]')?.getAttribute('content') ?? 'light dark';
    if (meta === 'dark') return true;
    if (meta === 'light') return false;
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  public rebuildThemeScene(dark: boolean) {
    this.scene.background = new THREE.Color(dark ? 0x1b1d24 : 0xf4f5f8);
  }

  public loadUrdfXml(xml: string, workingDirPath: string, modelKey: string) {
    const warnings: string[] = [];
    const manager = new THREE.LoadingManager();

    const isSameModel = this.currentModelKey === modelKey && this.robot !== null;
    this.currentModelKey = modelKey;

    if (this.robot) {
      for (const [name, j] of Object.entries(this.robot.joints)) {
        if (j.jointType !== 'fixed' && typeof j.angle === 'number') {
          this.savedJointValues.set(name, j.angle);
        }
      }
      this.disposeCurrentRobot();
    } else {
      this.hiddenLinks.clear();
    }

    const loader = new URDFLoader(manager);
    loader.parseVisual = true;
    loader.parseCollision = true;

    const normalizedDir = workingDirPath.replace(/\\/g, '/').replace(/\/+$/, '');
    loader.workingPath = normalizedDir ? `/fs${normalizedDir.startsWith('/') ? '' : '/'}${normalizedDir}/` : '';
    loader.packages = (pkg: string) => `/pkg/${encodeURIComponent(pkg)}`;

    loader.loadMeshCb = (path: string, mgr: THREE.LoadingManager, done: (obj: THREE.Object3D, err?: Error) => void) => {
      const cleanPath = path.split('?')[0];
      const onError = (err: unknown) => {
        const msg = `无法加载网格: ${decodeURIComponent(path)}`;
        if (!warnings.includes(msg)) warnings.push(msg);
        done(new THREE.Object3D(), err instanceof Error ? err : new Error(String(err)));
      };

      if (/\.stl$/i.test(cleanPath)) {
        new STLLoader(mgr).load(
          path,
          geom => {
            geom.computeVertexNormals();
            const mat = new THREE.MeshStandardMaterial({ color: 0xb8c2d1, roughness: 0.45, metalness: 0.15 });
            done(new THREE.Mesh(geom, mat));
          },
          undefined,
          onError
        );
      } else if (/\.dae$/i.test(cleanPath)) {
        new ColladaLoader(mgr).load(
          path,
          dae => done(dae.scene),
          undefined,
          onError
        );
      } else if (/\.obj$/i.test(cleanPath)) {
        new OBJLoader(mgr).load(
          path,
          obj => done(obj),
          undefined,
          onError
        );
      } else if (/\.(glb|gltf)$/i.test(cleanPath)) {
        new GLTFLoader(mgr).load(
          path,
          gltf => done(gltf.scene),
          undefined,
          onError
        );
      } else {
        onError(new Error(`不支持的网格格式: ${cleanPath}`));
      }
    };

    let parsedRobot: URDFRobot;
    try {
      parsedRobot = loader.parse(xml);
    } catch (e) {
      throw new Error(`URDF 解析失败: ${e instanceof Error ? e.message : String(e)}`);
    }

    this.robot = parsedRobot;
    this.scene.add(parsedRobot);

    // Restore joint angles across live edits
    for (const [name, joint] of Object.entries(parsedRobot.joints)) {
      if (joint.jointType !== 'fixed' && this.savedJointValues.has(name)) {
        joint.setJointValue(this.savedJointValues.get(name)!);
      }
    }

    this.buildOverlays(parsedRobot);
    this.styleRobotMaterials(parsedRobot);
    this.applyVisibility();

    const notifyLoaded = () => {
      if (this.robot !== parsedRobot) return;
      this.styleRobotMaterials(parsedRobot);
      this.applyVisibility();
      if (!isSameModel) {
        this.fitCameraToRobot();
      }
      this.callbacks.onRobotLoaded(this.collectRobotInfo(parsedRobot, warnings));
    };

    // Notify immediately for primitive-only URDFs, and again once async meshes finish
    queueMicrotask(() => {
      if (this.robot === parsedRobot) {
        this.callbacks.onRobotLoaded(this.collectRobotInfo(parsedRobot, warnings));
        if (!isSameModel) this.fitCameraToRobot();
      }
    });
    manager.onLoad = notifyLoaded;
    manager.onError = () => {
      if (this.robot === parsedRobot) {
        this.callbacks.onRobotLoaded(this.collectRobotInfo(parsedRobot, warnings));
      }
    };
  }

  private collectRobotInfo(robot: URDFRobot, warnings: string[]) {
    const joints: JointMeta[] = [];
    for (const j of Object.values(robot.joints)) {
      if (j.jointType === 'fixed' || j.jointType === 'floating') continue;
      const isCont = j.jointType === 'continuous';
      const lower = isCont ? -Math.PI : (Number.isFinite(j.limit?.lower) ? j.limit.lower : -Math.PI);
      const upper = isCont ? Math.PI : (Number.isFinite(j.limit?.upper) ? j.limit.upper : Math.PI);
      joints.push({
        name: j.name,
        jointType: j.jointType,
        lower: lower === upper ? -Math.PI : lower,
        upper: lower === upper ? Math.PI : upper,
        value: typeof j.angle === 'number' ? j.angle : 0
      });
    }

    const links: LinkMeta[] = Object.values(robot.links).map((l: any) => ({
      name: l.name,
      mass: l.inertial?.mass ?? 0,
      visible: !this.hiddenLinks.has(l.name)
    }));

    return {
      robotName: robot.robotName || 'robot',
      joints,
      links,
      meshWarnings: [...warnings]
    };
  }

  private styleRobotMaterials(robot: URDFRobot) {
    robot.traverse((obj: any) => {
      if (obj.isURDFCollider) {
        obj.traverse((child: any) => {
          if (child.isMesh && !child.userData.__collisionStyled) {
            child.userData.__collisionStyled = true;
            child.material = new THREE.MeshStandardMaterial({
              color: 0xf59e0b,
              transparent: true,
              opacity: 0.42,
              roughness: 0.4,
              depthWrite: false
            });
          }
        });
      } else if (obj.isMesh && !this.isInsideOverlay(obj) && !this.isInsideCollider(obj)) {
        obj.castShadow = true;
        obj.receiveShadow = true;
      }
    });
  }

  private isInsideCollider(obj: THREE.Object3D): boolean {
    let cur: THREE.Object3D | null = obj;
    while (cur) {
      if ((cur as any).isURDFCollider) return true;
      cur = cur.parent;
    }
    return false;
  }

  private isInsideOverlay(obj: THREE.Object3D): boolean {
    let cur: THREE.Object3D | null = obj;
    while (cur) {
      if (
        cur.name === '__urdf_inertia__' ||
        cur.name === '__urdf_tf__' ||
        cur.name === '__urdf_joint_axis__'
      ) {
        return true;
      }
      cur = cur.parent;
    }
    return false;
  }

  private buildOverlays(robot: URDFRobot) {
    this.tfHelpers = [];
    this.axisHelpers = [];
    this.inertiaHelpers = [];

    for (const link of Object.values(robot.links) as any[]) {
      const tf = new THREE.AxesHelper(1);
      tf.name = '__urdf_tf__';
      tf.scale.setScalar(this.tfScale);
      tf.layers.set(OVERLAY_LAYER);
      (tf.material as THREE.Material).depthTest = false;
      tf.renderOrder = 15;
      link.add(tf);
      this.tfHelpers.push(tf);

      if (link.inertial) {
        const inertiaGroup = createInertiaVisual(link.inertial, OVERLAY_LAYER);
        if (inertiaGroup) {
          link.add(inertiaGroup);
          this.inertiaHelpers.push(inertiaGroup);
        }
      }
    }

    for (const joint of Object.values(robot.joints) as any[]) {
      if (joint.jointType === 'fixed') continue;
      const dir = (joint.axis ? joint.axis.clone() : new THREE.Vector3(1, 0, 0)).normalize();
      const color = joint.jointType === 'prismatic' ? 0x06b6d4 : 0xfacc15;
      const arrow = new THREE.ArrowHelper(dir, new THREE.Vector3(0, 0, 0), this.tfScale * 1.35, color, this.tfScale * 0.32, this.tfScale * 0.18);
      arrow.name = '__urdf_joint_axis__';
      arrow.traverse(c => {
        c.layers.set(OVERLAY_LAYER);
        if ((c as any).material) {
          (c as any).material.depthTest = false;
        }
        c.renderOrder = 16;
      });
      joint.add(arrow);
      this.axisHelpers.push(arrow);
    }
  }

  public setTfScale(scale: number) {
    this.tfScale = scale;
    for (const tf of this.tfHelpers) {
      tf.scale.setScalar(scale);
    }
    for (const arrow of this.axisHelpers) {
      if (arrow instanceof THREE.ArrowHelper) {
        arrow.setLength(scale * 1.35, scale * 0.32, scale * 0.18);
      }
    }
  }

  public applyVisibility() {
    if (!this.robot) return;

    for (const [linkName, link] of Object.entries(this.robot.links)) {
      const linkAllowed = !this.hiddenLinks.has(linkName);
      for (const child of link.children as any[]) {
        if (child.isURDFVisual) {
          child.visible = linkAllowed && this.showVisual;
        } else if (child.isURDFCollider) {
          child.visible = linkAllowed && this.showCollision;
        } else if (child.name === '__urdf_inertia__') {
          child.visible = linkAllowed && this.showInertia;
        } else if (child.name === '__urdf_tf__') {
          child.visible = linkAllowed && this.showTf;
        }
      }
    }

    for (const arrow of this.axisHelpers) {
      arrow.visible = this.showJointAxes;
    }
  }

  public setLinkVisible(linkName: string, visible: boolean) {
    if (visible) {
      this.hiddenLinks.delete(linkName);
    } else {
      this.hiddenLinks.add(linkName);
    }
    this.applyVisibility();
  }

  public setAllLinksVisible(visible: boolean) {
    if (!this.robot) return;
    if (visible) {
      this.hiddenLinks.clear();
    } else {
      for (const name of Object.keys(this.robot.links)) {
        this.hiddenLinks.add(name);
      }
    }
    this.applyVisibility();
  }

  public setJointValue(name: string, val: number) {
    if (!this.robot || !(name in this.robot.joints)) return;
    this.robot.joints[name].setJointValue(val);
    this.savedJointValues.set(name, val);
  }

  public resetAllJoints() {
    if (!this.robot) return;
    for (const [name, j] of Object.entries(this.robot.joints)) {
      if (j.jointType !== 'fixed') {
        const target = Math.max(j.limit?.lower ?? 0, Math.min(j.limit?.upper ?? 0, 0));
        j.setJointValue(target);
        this.savedJointValues.set(name, target);
        this.callbacks.onJointDrag(name, target);
      }
    }
  }

  public randomizeJoints() {
    if (!this.robot) return;
    for (const [name, j] of Object.entries(this.robot.joints)) {
      if (j.jointType === 'fixed' || j.jointType === 'floating') continue;
      const lo = j.jointType === 'continuous' ? -Math.PI : (j.limit?.lower ?? -1.5);
      const hi = j.jointType === 'continuous' ? Math.PI : (j.limit?.upper ?? 1.5);
      const val = lo + Math.random() * (hi - lo);
      j.setJointValue(val);
      this.savedJointValues.set(name, val);
      this.callbacks.onJointDrag(name, val);
    }
  }

  public clearMeasurements() {
    this.measure.clear();
    this.measure.setPreviewPoint(null);
  }

  public fitCameraToRobot() {
    if (!this.robot) return;
    const box = new THREE.Box3();
    this.robot.traverse((obj: any) => {
      if (obj.isMesh && obj.visible && !this.isInsideOverlay(obj)) {
        box.expandByObject(obj);
      }
    });

    if (box.isEmpty()) {
      box.setFromCenterAndSize(new THREE.Vector3(0, 0, 0.25), new THREE.Vector3(0.8, 0.8, 0.8));
    }

    const center = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3());
    const maxDim = Math.max(0.25, size.x, size.y, size.z);
    const dist = maxDim * 1.85;

    this.orbit.target.copy(center);
    this.camera.position.set(center.x + dist * 0.85, center.y - dist * 0.85, center.z + dist * 0.65);
    this.camera.near = Math.max(0.001, maxDim * 0.005);
    this.camera.far = Math.max(100, maxDim * 80);
    this.camera.updateProjectionMatrix();
    this.orbit.update();
  }

  private findNearestMovableJoint(obj: THREE.Object3D | null): URDFJoint | null {
    let curr: any = obj;
    while (curr) {
      if (curr.isURDFJoint && curr.jointType !== 'fixed' && curr.jointType !== 'floating') {
        return curr as URDFJoint;
      }
      curr = curr.parent;
    }
    return null;
  }

  private findNearestLinkName(obj: THREE.Object3D | null): string | null {
    let curr: any = obj;
    while (curr) {
      if (curr.isURDFLink && curr.name) {
        return curr.name;
      }
      curr = curr.parent;
    }
    return null;
  }

  private updatePointerRay(e: PointerEvent) {
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.pointer.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    this.pointer.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
    this.raycaster.setFromCamera(this.pointer, this.camera);
  }

  private raycastRobotSurface(): THREE.Intersection | null {
    if (!this.robot) return null;
    const hits = this.raycaster.intersectObject(this.robot, true);
    for (const h of hits) {
      if (!this.isInsideOverlay(h.object)) {
        return h;
      }
    }
    return null;
  }

  private computeJointDelta(joint: any, startPt: THREE.Vector3, endPt: THREE.Vector3): number {
    if (joint.jointType === 'prismatic') {
      const axisWorld = joint.axis.clone().transformDirection(joint.parent.matrixWorld).normalize();
      return endPt.clone().sub(startPt).dot(axisWorld);
    }
    if (joint.jointType === 'revolute' || joint.jointType === 'continuous') {
      const axisWorld = joint.axis.clone().transformDirection(joint.matrixWorld).normalize();
      const pivot = new THREE.Vector3(0, 0, 0).applyMatrix4(joint.matrixWorld);
      const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(axisWorld, pivot);

      const pStart = plane.projectPoint(startPt, new THREE.Vector3()).sub(pivot);
      const pEnd = plane.projectPoint(endPt, new THREE.Vector3()).sub(pivot);
      if (pStart.lengthSq() < 1e-8 || pEnd.lengthSq() < 1e-8) return 0;

      const cross = new THREE.Vector3().crossVectors(pStart, pEnd);
      const sign = Math.sign(cross.dot(axisWorld));
      return sign * pEnd.angleTo(pStart);
    }
    return 0;
  }

  private bindPointerEvents() {
    const dom = this.renderer.domElement;

    dom.addEventListener('pointerdown', e => {
      if (e.button !== 0) return;
      this.pointerDownPos.set(e.clientX, e.clientY);
      this.updatePointerRay(e);
      const hit = this.raycastRobotSurface();

      if (this.interactionMode === 'drag' && hit) {
        const joint = this.findNearestMovableJoint(hit.object);
        if (joint) {
          this.draggingJoint = joint;
          this.dragHitDistance = hit.distance;
          this.initialGrabPoint.copy(hit.point);
          this.prevRay.copy(this.raycaster.ray);
          this.orbit.enabled = false;
          dom.setPointerCapture(e.pointerId);
        }
      }
    });

    dom.addEventListener('pointermove', e => {
      this.updatePointerRay(e);

      if (this.draggingJoint) {
        const prevPt = this.prevRay.at(this.dragHitDistance, new THREE.Vector3());
        const currPt = this.raycaster.ray.at(this.dragHitDistance, new THREE.Vector3());
        const delta = this.computeJointDelta(this.draggingJoint, prevPt, currPt);
        if (Math.abs(delta) > 1e-6) {
          let nextVal = (this.draggingJoint.angle || 0) + delta;
          if (this.draggingJoint.jointType === 'revolute' || this.draggingJoint.jointType === 'prismatic') {
            const lo = this.draggingJoint.limit?.lower ?? -Math.PI;
            const hi = this.draggingJoint.limit?.upper ?? Math.PI;
            if (lo < hi) nextVal = Math.max(lo, Math.min(hi, nextVal));
          }
          this.draggingJoint.setJointValue(nextVal);
          this.savedJointValues.set(this.draggingJoint.name, this.draggingJoint.angle);
          this.callbacks.onJointDrag(this.draggingJoint.name, this.draggingJoint.angle);
        }
        this.prevRay.copy(this.raycaster.ray);
        return;
      }

      const hit = this.raycastRobotSurface();
      if (hit) {
        const linkName = this.findNearestLinkName(hit.object);
        this.callbacks.onHoverPoint(hit.point, linkName);
        if (this.interactionMode === 'distance' || this.interactionMode === 'angle') {
          this.measure.setPreviewPoint(hit.point, this.camera.position.distanceTo(hit.point));
          dom.style.cursor = 'crosshair';
        } else {
          this.measure.setPreviewPoint(null);
          dom.style.cursor = this.findNearestMovableJoint(hit.object) ? 'grab' : 'default';
        }
      } else {
        this.callbacks.onHoverPoint(null, null);
        this.measure.setPreviewPoint(null);
        dom.style.cursor = 'default';
      }
    });

    dom.addEventListener('pointerup', e => {
      if (this.draggingJoint) {
        this.draggingJoint = null;
        this.orbit.enabled = true;
        if (dom.hasPointerCapture(e.pointerId)) {
          dom.releasePointerCapture(e.pointerId);
        }
      }

      const movedDist = Math.hypot(e.clientX - this.pointerDownPos.x, e.clientY - this.pointerDownPos.y);
      if (movedDist < 4 && e.button === 0) {
        this.updatePointerRay(e);
        const hit = this.raycastRobotSurface();
        if (hit) {
          if (this.interactionMode === 'distance' || this.interactionMode === 'angle') {
            const res = this.measure.addPoint(
              hit.point,
              this.interactionMode,
              this.camera.position.distanceTo(hit.point)
            );
            this.callbacks.onMeasureUpdate(res);
          } else {
            this.callbacks.onSelectLink(this.findNearestLinkName(hit.object));
          }
        }
      }
    });
  }

  private disposeCurrentRobot() {
    if (!this.robot) return;
    this.scene.remove(this.robot);
    this.robot.traverse((obj: any) => {
      obj.geometry?.dispose?.();
      if (Array.isArray(obj.material)) {
        obj.material.forEach((m: any) => m?.dispose?.());
      } else {
        obj.material?.dispose?.();
      }
    });
    this.robot = null;
  }

  private handleResize() {
    const w = Math.max(1, this.container.clientWidth);
    const h = Math.max(1, this.container.clientHeight);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h, false);
  }

  private animate = () => {
    requestAnimationFrame(this.animate);
    this.orbit.update();
    this.measure.updateLabels(
      this.camera,
      this.container.clientWidth,
      this.container.clientHeight
    );
    this.renderer.render(this.scene, this.camera);
  };
}
