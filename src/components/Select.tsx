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
  // 键盘导航的高亮索引（↑↓ 移动、Enter 选择）。打开时默认指向当前选中项。
  const [highlight, setHighlight] = useState(-1);
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

  // 打开时把高亮重置到当前选中项；关闭时复位。
  useEffect(() => {
    if (open) {
      const idx = options.findIndex((o) => o.value === value);
      setHighlight(idx >= 0 ? idx : 0);
    } else {
      setHighlight(-1);
    }
  }, [open, options, value]);

  function onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>) {
    if (disabled) return;
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => {
        for (let i = h + 1; i < options.length; i++) if (!options[i].disabled) return i;
        return h < 0 && options.length ? options.findIndex((o) => !o.disabled) : h;
      });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => {
        for (let i = h - 1; i >= 0; i--) if (!options[i].disabled) return i;
        return h;
      });
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = highlight >= 0 ? options[highlight] : undefined;
      if (opt && !opt.disabled) {
        onChange(opt.value);
        setOpen(false);
      }
    }
  }

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={onKeyDown}
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
          role="listbox"
          aria-activedescendant={highlight >= 0 ? `sel-opt-${highlight}` : undefined}
          className={`cf-menu-in cf-dropdown absolute z-30 max-h-[300px] min-w-full overflow-y-auto p-1 ${
            align === "right" ? "right-0" : "left-0"
          } ${direction === "up" ? "bottom-[calc(100%+4px)]" : "top-[calc(100%+4px)]"}`}
        >
          {options.map((o, i) => {
            const active = o.value === value;
            const highlighted = i === highlight;
            return (
              <button
                key={o.value}
                id={`sel-opt-${i}`}
                type="button"
                role="option"
                aria-selected={active}
                disabled={o.disabled}
                onMouseEnter={() => !o.disabled && setHighlight(i)}
                onClick={() => {
                  if (o.disabled) return;
                  onChange(o.value);
                  setOpen(false);
                }}
                className={`cf-menu-item flex w-full items-center gap-2 ${active ? "is-active" : ""} ${
                  highlighted ? "is-highlighted" : ""
                } ${o.disabled ? "cursor-not-allowed opacity-45" : ""}`}
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
