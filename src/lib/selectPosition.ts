export type SelectDirection = "auto" | "down" | "up";
export type SelectAlignment = "left" | "right";

interface TriggerGeometry {
  top: number;
  right: number;
  bottom: number;
  left: number;
  width: number;
}

interface MenuGeometry {
  width: number;
  height: number;
}

interface ViewportGeometry {
  width: number;
  height: number;
}

export interface SelectMenuPlacementInput {
  trigger: TriggerGeometry;
  menu: MenuGeometry;
  viewport: ViewportGeometry;
  align: SelectAlignment;
  direction: SelectDirection;
  gap?: number;
  viewportPadding?: number;
  heightCap?: number;
}

export interface SelectMenuPlacement {
  direction: "down" | "up";
  top: number;
  left: number;
  maxHeight: number;
  minWidth: number;
  maxWidth: number;
}

export function computeSelectMenuPlacement({
  trigger,
  menu,
  viewport,
  align,
  direction,
  gap = 4,
  viewportPadding = 8,
  heightCap = 300,
}: SelectMenuPlacementInput): SelectMenuPlacement {
  const spaceAbove = Math.max(0, trigger.top - gap - viewportPadding);
  const spaceBelow = Math.max(0, viewport.height - trigger.bottom - gap - viewportPadding);
  const desiredHeight = Math.min(menu.height, heightCap);
  const resolvedDirection =
    direction === "auto"
      ? desiredHeight <= spaceBelow || spaceBelow >= spaceAbove
        ? "down"
        : "up"
      : direction;
  const availableHeight = resolvedDirection === "up" ? spaceAbove : spaceBelow;
  const maxHeight = Math.min(desiredHeight, availableHeight);
  const maxWidth = Math.max(0, viewport.width - viewportPadding * 2);
  const resolvedWidth = Math.min(Math.max(menu.width, trigger.width), maxWidth);
  const preferredLeft = align === "right" ? trigger.right - resolvedWidth : trigger.left;
  const left = Math.min(
    Math.max(viewportPadding, preferredLeft),
    Math.max(viewportPadding, viewport.width - viewportPadding - resolvedWidth),
  );
  const top =
    resolvedDirection === "up"
      ? Math.max(viewportPadding, trigger.top - gap - maxHeight)
      : trigger.bottom + gap;

  return {
    direction: resolvedDirection,
    top,
    left,
    maxHeight,
    minWidth: Math.min(trigger.width, maxWidth),
    maxWidth,
  };
}
