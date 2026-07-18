import { useMemo } from "react";
import { Canvas } from "@react-three/fiber";
import * as THREE from "three";
import { useModelStore } from "../state/store";
import { buildMesh } from "../geometry/buildMesh";

function SolidMesh() {
  const features = useModelStore((s) => s.features);
  const geometry = useMemo(() => {
    const { positions, indices } = buildMesh(features);
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    g.setIndex(new THREE.BufferAttribute(indices, 1));
    g.computeVertexNormals();
    return g;
  }, [features]);

  return (
    <mesh geometry={geometry}>
      <meshStandardMaterial color="#4f9dff" metalness={0.1} roughness={0.5} />
    </mesh>
  );
}

export function Viewport() {
  const setSelected = useModelStore((s) => s.setSelected);
  return (
    <div className="viewport" data-testid="viewport">
      <Canvas camera={{ position: [60, 60, 60], fov: 50 }} onClick={() => setSelected("solid")}>
        <ambientLight intensity={0.6} />
        <directionalLight position={[50, 80, 40]} intensity={1.0} />
        <directionalLight position={[-40, -20, -30]} intensity={0.3} />
        <SolidMesh />
        <gridHelper args={[200, 20, "#2a2a35", "#1a1a22"]} />
      </Canvas>
    </div>
  );
}
