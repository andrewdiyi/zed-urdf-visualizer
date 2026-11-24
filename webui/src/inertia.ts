import * as THREE from 'three';

export interface InertialData {
  mass: number;
  origin: {
    xyz: [number, number, number];
    rpy: [number, number, number];
  };
  inertia: {
    ixx: number;
    ixy: number;
    ixz: number;
    iyy: number;
    iyz: number;
    izz: number;
  };
}

/**
 * Exact 3x3 symmetric Jacobi eigensolver for URDF inertia tensors.
 * Returns eigenvalues [I1, I2, I3] and a right-handed rotation Matrix4
 * whose columns are the corresponding principal axes.
 */
function eigenSymmetric3x3(
  a00: number,
  a01: number,
  a02: number,
  a11: number,
  a12: number,
  a22: number
): { values: [number, number, number]; basis: THREE.Matrix4 } {
  const A = [
    [a00, a01, a02],
    [a01, a11, a12],
    [a02, a12, a22]
  ];
  const V = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1]
  ];

  for (let iter = 0; iter < 25; iter++) {
    let p = 0;
    let q = 1;
    let maxOff = Math.abs(A[0][1]);
    if (Math.abs(A[0][2]) > maxOff) {
      maxOff = Math.abs(A[0][2]);
      p = 0;
      q = 2;
    }
    if (Math.abs(A[1][2]) > maxOff) {
      maxOff = Math.abs(A[1][2]);
      p = 1;
      q = 2;
    }
    if (maxOff < 1e-12) break;

    const app = A[p][p];
    const aqq = A[q][q];
    const apq = A[p][q];
    const tau = (aqq - app) / (2 * apq);
    const t = tau >= 0
      ? 1 / (tau + Math.sqrt(1 + tau * tau))
      : -1 / (-tau + Math.sqrt(1 + tau * tau));
    const c = 1 / Math.sqrt(1 + t * t);
    const s = t * c;

    A[p][p] = app - t * apq;
    A[q][q] = aqq + t * apq;
    A[p][q] = 0;
    A[q][p] = 0;

    for (let r = 0; r < 3; r++) {
      if (r !== p && r !== q) {
        const arp = A[r][p];
        const arq = A[r][q];
        A[r][p] = c * arp - s * arq;
        A[p][r] = A[r][p];
        A[r][q] = s * arp + c * arq;
        A[q][r] = A[r][q];
      }
      const vrp = V[r][p];
      const vrq = V[r][q];
      V[r][p] = c * vrp - s * vrq;
      V[r][q] = s * vrp + c * vrq;
    }
  }

  const col0 = new THREE.Vector3(V[0][0], V[1][0], V[2][0]).normalize();
  const col1 = new THREE.Vector3(V[0][1], V[1][1], V[2][1]).normalize();
  const col2 = new THREE.Vector3().crossVectors(col0, col1).normalize();

  const basis = new THREE.Matrix4().makeBasis(col0, col1, col2);
  return {
    values: [A[0][0], A[1][1], A[2][2]],
    basis
  };
}

/**
 * Builds a ROS RViz / Gazebo-style inertia visualization Group (on Layer 1)
 * showing the center-of-mass sphere and principal-axes equivalent homogeneous box.
 */
export function createInertiaVisual(inertial: InertialData, overlayLayer = 1): THREE.Group | null {
  const mass = inertial?.mass ?? 0;
  if (!(mass > 0)) return null;

  const { ixx, ixy, ixz, iyy, iyz, izz } = inertial.inertia;
  const trace = ixx + iyy + izz;
  if (!(trace > 0)) return null;

  const root = new THREE.Group();
  root.name = '__urdf_inertia__';

  const xyz = inertial.origin?.xyz ?? [0, 0, 0];
  const rpy = inertial.origin?.rpy ?? [0, 0, 0];
  root.position.set(xyz[0] || 0, xyz[1] || 0, xyz[2] || 0);
  const euler = new THREE.Euler(rpy[0] || 0, rpy[1] || 0, rpy[2] || 0, 'ZYX');
  root.quaternion.setFromEuler(euler);

  const { values: [I1, I2, I3], basis } = eigenSymmetric3x3(ixx, ixy, ixz, iyy, iyz, izz);

  // Equivalent uniform-density box dimensions: lx = sqrt(6/m * (I2 + I3 - I1))
  const minDim = 0.008;
  const lx = Math.sqrt(Math.max(minDim * minDim, (6 / mass) * Math.max(0, I2 + I3 - I1)));
  const ly = Math.sqrt(Math.max(minDim * minDim, (6 / mass) * Math.max(0, I1 + I3 - I2)));
  const lz = Math.sqrt(Math.max(minDim * minDim, (6 / mass) * Math.max(0, I1 + I2 - I3)));

  const principalGroup = new THREE.Group();
  principalGroup.quaternion.setFromRotationMatrix(basis);

  const boxGeom = new THREE.BoxGeometry(lx, ly, lz);
  const boxMat = new THREE.MeshStandardMaterial({
    color: 0xa855f7,
    transparent: true,
    opacity: 0.34,
    roughness: 0.4,
    depthWrite: false
  });
  const boxMesh = new THREE.Mesh(boxGeom, boxMat);
  boxMesh.layers.set(overlayLayer);
  principalGroup.add(boxMesh);

  const edges = new THREE.LineSegments(
    new THREE.EdgesGeometry(boxGeom),
    new THREE.LineBasicMaterial({ color: 0xc084fc, transparent: true, opacity: 0.85 })
  );
  edges.layers.set(overlayLayer);
  principalGroup.add(edges);

  const comRadius = Math.max(0.006, Math.min(lx, ly, lz) * 0.18);
  const comSphere = new THREE.Mesh(
    new THREE.SphereGeometry(comRadius, 16, 12),
    new THREE.MeshBasicMaterial({ color: 0xf43f5e, depthTest: false })
  );
  comSphere.renderOrder = 10;
  comSphere.layers.set(overlayLayer);

  root.add(principalGroup);
  root.add(comSphere);
  root.layers.set(overlayLayer);

  return root;
}
