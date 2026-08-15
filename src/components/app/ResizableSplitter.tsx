import { useCallback } from "react";
import type { RefObject } from "react";
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
  previewTargetRef: RefObject<HTMLElement | null>;
  previewCssVariable: `--${string}`;
  onCommit: (value: number) => void;
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
  previewTargetRef,
  previewCssVariable,
  onCommit,
  onReset,
}: ResizableSplitterProps) {
  const onPreview = useCallback(
    (nextValue: number) => {
      previewTargetRef.current?.style.setProperty(previewCssVariable, `${nextValue}px`);
    },
    [previewCssVariable, previewTargetRef],
  );
  const { separatorProps } = useResizablePane({
    value,
    min,
    max,
    direction,
    onPreview,
    onCommit,
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
