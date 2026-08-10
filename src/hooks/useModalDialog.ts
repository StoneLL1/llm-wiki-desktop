import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE_SELECTOR = [
  "button:not(:disabled)",
  "input:not(:disabled)",
  "textarea:not(:disabled)",
  "select:not(:disabled)",
  "a[href]",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

interface UseModalDialogOptions {
  open?: boolean;
  onClose: () => void;
  initialFocusRef?: RefObject<HTMLElement | null>;
  returnFocusSelector?: string;
}

export function useModalDialog<T extends HTMLElement = HTMLDivElement>({
  open = true,
  onClose,
  initialFocusRef,
  returnFocusSelector,
}: UseModalDialogOptions) {
  const dialogRef = useRef<T>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;

    const dialog = dialogRef.current;
    const trigger = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const focusable = () => dialog
      ? Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
      : [];

    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    (initialFocusRef?.current ?? focusable()[0] ?? dialog)?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        // Only the topmost modal reacts, so dismissing a nested dialog
        // doesn't also close the one beneath it.
        const modals = document.querySelectorAll("[aria-modal='true']");
        if (modals.length > 0 && modals[modals.length - 1] !== dialog) return;
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;

      const elements = focusable();
      if (elements.length === 0) {
        event.preventDefault();
        dialog?.focus();
        return;
      }
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    const keepFocusInside = (event: FocusEvent) => {
      if (!dialog || dialog.contains(event.target as Node)) return;
      (initialFocusRef?.current ?? focusable()[0] ?? dialog).focus();
    };

    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("focusin", keepFocusInside);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("focusin", keepFocusInside);
      document.body.style.overflow = prevOverflow;
      const returnTarget = returnFocusSelector
        ? document.querySelector<HTMLElement>(returnFocusSelector)
        : null;
      (returnTarget ?? trigger)?.focus();
    };
  }, [initialFocusRef, open, returnFocusSelector]);

  return dialogRef;
}
