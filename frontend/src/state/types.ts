export type FeatureType =
  | "sketch"
  | "extrude"
  | "revolve"
  | "boolean"
  | "fillet"
  | "chamfer";

export interface FeatureNode {
  id: string;
  type: FeatureType;
  label: string;
  params: Record<string, number | string>;
}

export interface AssemblyNode {
  id: string;
  name: string;
  children: AssemblyNode[];
}

export interface ModelState {
  features: FeatureNode[];
  assemblies: AssemblyNode[];
}

export interface SelectionState {
  selectedFeatureId: string | null;
  hoveredFeatureId: string | null;
}
