import type { ResizablePaneId } from "../../hooks/useResizablePane";
import { useResizablePane } from "../../hooks/useResizablePane";

export interface ResizableSplitterProps {
  paneId: ResizablePaneId;
  label: string;
  min: number;
  max: number;
  value: number;
  direction?: 1 | -1;
  className?: string;
  onChange: (value: number) => void;
  onReset: () => void;
}

export function ResizableSplitter({
  paneId,
  label,
  min,
  max,
  value,
  direction = 1,
  className,
  onChange,
  onReset,
}: ResizableSplitterProps) {
  const { separatorProps } = useResizablePane({
    value,
    min,
    max,
    direction,
    onChange,
    onReset,
  });

  return (
    <div
      {...separatorProps}
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemax={max}
      aria-valuemin={min}
      aria-valuenow={value}
      className={["resize-handle", className].filter(Boolean).join(" ")}
      data-pane-id={paneId}
      role="separator"
      tabIndex={0}
    />
  );
}
