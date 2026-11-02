import type { ReactNode } from "react";
import { Dialog as D } from "radix-ui";
import { X } from "lucide-react";
import { IconButton } from "./IconButton";
import { Button } from "./Button";

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
}

export function Dialog({ open, onOpenChange, title, description, children, footer }: DialogProps) {
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-(--z-scrim) bg-scrim data-[state=closed]:animate-fade-out data-[state=open]:animate-fade-in" />
        <D.Content className="fixed top-1/2 left-1/2 z-(--z-dialog) grid max-h-[85vh] w-[min(480px,calc(100vw-48px))] -translate-1/2 gap-5 overflow-y-auto rounded-xl bg-surface p-5 shadow-3 focus:outline-none data-[state=closed]:animate-dialog-out data-[state=open]:animate-dialog-in">
          <header className="flex items-start justify-between gap-4">
            <div>
              <D.Title className="text-base font-semibold text-fg">{title}</D.Title>
              {description ? (
                <D.Description className="mt-1 text-sm text-fg-muted">{description}</D.Description>
              ) : (
                <D.Description className="sr-only">{title}</D.Description>
              )}
            </div>
            <D.Close asChild>
              <IconButton label="Close" icon={<X />} />
            </D.Close>
          </header>
          {children}
          {footer && <footer className="flex justify-end gap-2">{footer}</footer>}
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}

/** Yes/no confirmation for destructive actions. */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  onConfirm,
}: Omit<DialogProps, "children" | "footer"> & { confirmLabel: string; onConfirm: () => void }) {
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description={description}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              onConfirm();
              onOpenChange(false);
            }}
          >
            {confirmLabel}
          </Button>
        </>
      }
    />
  );
}
