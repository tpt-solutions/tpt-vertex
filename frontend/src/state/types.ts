export interface FeatureNode {
  id: string;
  type: "extrude" | "revolve" | "sketch" | "boolean";
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
