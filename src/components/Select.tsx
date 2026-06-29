import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDownIcon, CheckIcon } from "./Icons";

export interface SelectOption {
  value: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
}

/**
 * Animated dropdown: button trigger + popover list.
 * Click-outside / Escape to close, selected item checked. Use instead of native <select>.
 */
export function Select({
  value,
  options,
  onChange,
  placeholder = "Select",
  triggerClassName,
  buttonContent,
  align = "left",
  direction = "down",
  disabled,
}: {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  triggerClassName?: string;
  buttonContent?: ReactNode;
  align?: "left" | "right";
  direction?: "down" | "up";
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement | null>(null);
  const selected = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={
          triggerClassName ??
          "flex h-9 w-full items-center gap-2 rounded-md border border-[#d9d9d9] bg-white px-3 text-[13px] text-[#202124] outline-none transition hover:border-[#bcc2cb] focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10 disabled:cursor-not-allowed disabled:opacity-50"
        }
      >
        {selected?.icon && <span className="shrink-0">{selected.icon}</span>}
        <span className="min-w-0 flex-1 truncate text-left">
          {buttonContent ?? selected?.label ?? <span className="text-[#9aa1ab]">{placeholder}</span>}
        </span>
        <ChevronDownIcon
          size={16}
          className={`shrink-0 text-[#9aa1ab] transition-transform duration-150 ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div
          className={`cf-menu-in absolute z-30 max-h-[300px] min-w-full overflow-y-auto rounded-lg border border-[#e2e5ea] bg-white p-1 shadow-[0_12px_36px_rgba(15,23,42,0.16)] ${
            align === "right" ? "right-0" : "left-0"
          } ${direction === "up" ? "bottom-[calc(100%+4px)]" : "top-[calc(100%+4px)]"}`}
        >
          {options.map((o) => {
            const active = o.value === value;
            return (
              <button
                key={o.value}
                type="button"
                onClick={() => {
                  onChange(o.value);
                  setOpen(false);
                }}
                className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition ${
                  active ? "bg-[#eef1f5] text-[#111827]" : "text-[#3f444c] hover:bg-[#f3f4f7]"
                }`}
              >
                {o.icon && <span className="shrink-0">{o.icon}</span>}
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-medium">{o.label}</span>
                  {o.hint && <span className="mt-0.5 block truncate text-[11px] text-[#8a9099]">{o.hint}</span>}
                </span>
                {active && <CheckIcon size={15} className="shrink-0 text-[#111827]" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default Select;
