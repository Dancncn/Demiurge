import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDownIcon, CheckIcon } from "./Icons";

export interface SelectOption {
  value: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
  disabled?: boolean;
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
        className={`cf-press ${
          triggerClassName ??
          "flex h-8 w-full items-center gap-1.5 rounded-lg border border-[#e4e7ec] bg-[#fbfcfd] px-2.5 text-[13px] text-[#202124] outline-none transition hover:border-[#cfd5dd] hover:bg-white focus:border-[#bcc2cb] focus:bg-white focus:shadow-[0_0_0_3px_rgba(17,24,39,0.06)] disabled:cursor-not-allowed disabled:opacity-50"
        }`}
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
          className={`cf-menu-in cf-dropdown absolute z-30 max-h-[300px] min-w-full overflow-y-auto p-1 ${
            align === "right" ? "right-0" : "left-0"
          } ${direction === "up" ? "bottom-[calc(100%+4px)]" : "top-[calc(100%+4px)]"}`}
        >
          {options.map((o) => {
            const active = o.value === value;
            return (
              <button
                key={o.value}
                type="button"
                disabled={o.disabled}
                onClick={() => {
                  if (o.disabled) return;
                  onChange(o.value);
                  setOpen(false);
                }}
                className={`cf-menu-item flex w-full items-center gap-2 ${active ? "is-active" : ""} ${
                  o.disabled ? "cursor-not-allowed opacity-45" : ""
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
