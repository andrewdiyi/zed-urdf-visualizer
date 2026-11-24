import * as THREE from 'three';

export type InteractionMode = 'drag' | 'distance' | 'angle';

export interface MeasureResult {
  mode: 'distance' | 'angle';
  points: THREE.Vector3[];
  distance?: number;
  delta?: THREE.Vector3;
  angleRad?: number;
  angleDeg?: number;
}

interface LabelAnchor {
  position: THREE.Vector3;
  element: HTMLDivElement;
}

export class MeasurementController {
  private group = new THREE.Group();
  private points: THREE.Vector3[] = [];
  private labels: LabelAnchor[] = [];
  private previewMarker: THREE.Mesh;
  private overlayLayer: number;
  private labelsContainer: HTMLElement;

  constructor(scene: THREE.Scene, labelsContainer: HTMLElement, overlayLayer = 1) {
    this.overlayLayer = overlayLayer;
    this.labelsContainer = labelsContainer;
    this.group.name = '__urdf_measurements__';
    this.group.layers.set(overlayLayer);
    scene.add(this.group);

    this.previewMarker = new THREE.Mesh(
      new THREE.SphereGeometry(0.008, 16, 12),
      new THREE.MeshBasicMaterial({ color: 0x38bdf8, depthTest: false, transparent: true, opacity: 0.85 })
    );
    this.previewMarker.renderOrder = 20;
    this.previewMarker.visible = false;
    this.previewMarker.layers.set(overlayLayer);
    scene.add(this.previewMarker);
  }

  setPreviewPoint(pt: THREE.Vector3 | null, cameraDistance = 1.5) {
    if (!pt) {
      this.previewMarker.visible = false;
      return;
    }
    this.previewMarker.visible = true;
    this.previewMarker.position.copy(pt);
    const scale = Math.max(0.4, Math.min(3.5, cameraDistance * 0.6));
    this.previewMarker.scale.setScalar(scale);
  }

  clear() {
    this.points = [];
    while (this.group.children.length > 0) {
      const child = this.group.children.pop()!;
      child.traverse((obj: any) => {
        obj.geometry?.dispose?.();
        obj.material?.dispose?.();
      });
    }
    for (const lbl of this.labels) {
      lbl.element.remove();
    }
    this.labels = [];
  }

  addPoint(pt: THREE.Vector3, mode: 'distance' | 'angle', cameraDistance = 1.5): MeasureResult {
    const needed = mode === 'distance' ? 2 : 3;
    if (this.points.length >= needed) {
      this.clear();
    }
    this.points.push(pt.clone());

    const markerRadius = Math.max(0.006, Math.min(0.025, cameraDistance * 0.007));
    const marker = new THREE.Mesh(
      new THREE.SphereGeometry(markerRadius, 16, 12),
      new THREE.MeshBasicMaterial({
        color: this.points.length === 2 && mode === 'angle' ? 0xf59e0b : 0x22c55e,
        depthTest: false
      })
    );
    marker.position.copy(pt);
    marker.renderOrder = 22;
    marker.layers.set(this.overlayLayer);
    this.group.add(marker);

    this.addFloatingLabel(pt, `P${this.points.length}`);

    if (mode === 'distance' && this.points.length === 2) {
      const [a, b] = this.points;
      this.addSegment(a, b, 0x38bdf8);
      const dist = a.distanceTo(b);
      const mid = a.clone().add(b).multiplyScalar(0.5);
      this.addFloatingLabel(mid, `${(dist * 1000).toFixed(1)} mm (${dist.toFixed(4)} m)`);
      return {
        mode: 'distance',
        points: this.points.map(p => p.clone()),
        distance: dist,
        delta: b.clone().sub(a)
      };
    }

    if (mode === 'angle' && this.points.length >= 2) {
      const prev = this.points[this.points.length - 2];
      const curr = this.points[this.points.length - 1];
      this.addSegment(prev, curr, 0xf59e0b);

      if (this.points.length === 3) {
        const [a, b, c] = this.points;
        const ba = a.clone().sub(b);
        const bc = c.clone().sub(b);
        const angleRad = ba.angleTo(bc);
        const angleDeg = THREE.MathUtils.radToDeg(angleRad);
        this.addArc(b, ba, bc, Math.min(ba.length(), bc.length()) * 0.35);
        this.addFloatingLabel(b, `${angleDeg.toFixed(2)}° (${angleRad.toFixed(4)} rad)`);
        return {
          mode: 'angle',
          points: this.points.map(p => p.clone()),
          angleRad,
          angleDeg
        };
      }
    }

    return {
      mode,
      points: this.points.map(p => p.clone())
    };
  }

  private addSegment(a: THREE.Vector3, b: THREE.Vector3, color: number) {
    const geom = new THREE.BufferGeometry().setFromPoints([a, b]);
    const mat = new THREE.LineBasicMaterial({ color, depthTest: false });
    const line = new THREE.Line(geom, mat);
    line.renderOrder = 21;
    line.layers.set(this.overlayLayer);
    this.group.add(line);
  }

  private addArc(vertex: THREE.Vector3, v1: THREE.Vector3, v2: THREE.Vector3, radius: number) {
    if (radius <= 1e-5) return;
    const u1 = v1.clone().normalize();
    const u2 = v2.clone().normalize();
    const angle = u1.angleTo(u2);
    const axis = new THREE.Vector3().crossVectors(u1, u2);
    if (axis.lengthSq() < 1e-10) return;
    axis.normalize();

    const segments = 24;
    const arcPts: THREE.Vector3[] = [];
    for (let i = 0; i <= segments; i++) {
      const t = (i / segments) * angle;
      const dir = u1.clone().applyAxisAngle(axis, t).multiplyScalar(radius);
      arcPts.push(vertex.clone().add(dir));
    }
    const geom = new THREE.BufferGeometry().setFromPoints(arcPts);
    const line = new THREE.Line(
      geom,
      new THREE.LineBasicMaterial({ color: 0xfacc15, depthTest: false })
    );
    line.renderOrder = 21;
    line.layers.set(this.overlayLayer);
    this.group.add(line);
  }

  private addFloatingLabel(worldPos: THREE.Vector3, text: string) {
    const el = document.createElement('div');
    el.className = 'floating-label';
    el.textContent = text;
    this.labelsContainer.appendChild(el);
    this.labels.push({ position: worldPos.clone(), element: el });
  }

  updateLabels(camera: THREE.Camera, width: number, height: number) {
    const proj = new THREE.Vector3();
    for (const lbl of this.labels) {
      proj.copy(lbl.position).project(camera);
      if (proj.z > 1) {
        lbl.element.style.display = 'none';
        continue;
      }
      lbl.element.style.display = 'block';
      const x = (proj.x * 0.5 + 0.5) * width;
      const y = (-proj.y * 0.5 + 0.5) * height;
      lbl.element.style.left = `${x}px`;
      lbl.element.style.top = `${y}px`;
    }
  }
}
