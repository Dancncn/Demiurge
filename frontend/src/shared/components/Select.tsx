import { createPortal } from "react-dom";
import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import {
  computeSelectMenuPlacement,
  type SelectAlignment,
  type SelectDirection,
  type SelectMenuPlacement,
} from "@/lib/selectPosition";
import { ChevronDownIcon, CheckIcon } from "@/shared/components/Icons";

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
  direction = "auto",
  disabled,
}: {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  triggerClassName?: string;
  buttonContent?: ReactNode;
  align?: SelectAlignment;
  direction?: SelectDirection;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  // 键盘导航的高亮索引（↑↓ 移动、Enter 选择）。打开时默认指向当前选中项。
  const [highlight, setHighlight] = useState(-1);
  const [placement, setPlacement] = useState<SelectMenuPlacement | null>(null);
  const ref = useRef<HTMLDivElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const listboxId = useId();
  const selected = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (!ref.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false);
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

  useLayoutEffect(() => {
    if (!open) {
      setPlacement(null);
      return;
    }

    const trigger = ref.current;
    const menu = menuRef.current;
    if (!trigger || !menu) return;

    const updatePlacement = () => {
      const triggerRect = trigger.getBoundingClientRect();
      const menuRect = menu.getBoundingClientRect();
      const next = computeSelectMenuPlacement({
        trigger: triggerRect,
        menu: {
          width: Math.max(menuRect.width, triggerRect.width),
          height: menu.scrollHeight,
        },
        viewport: {
          width: document.documentElement.clientWidth,
          height: document.documentElement.clientHeight,
        },
        align,
        direction,
      });
      setPlacement((current) =>
        current &&
        current.direction === next.direction &&
        current.top === next.top &&
        current.left === next.left &&
        current.maxHeight === next.maxHeight &&
        current.minWidth === next.minWidth &&
        current.maxWidth === next.maxWidth
          ? current
          : next,
      );
    };

    updatePlacement();
    const observer = new ResizeObserver(updatePlacement);
    observer.observe(trigger);
    observer.observe(menu);
    window.addEventListener("resize", updatePlacement);
    window.addEventListener("scroll", updatePlacement, true);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", updatePlacement);
      window.removeEventListener("scroll", updatePlacement, true);
    };
  }, [align, direction, open, options]);

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
    <div ref={ref} className="md-select relative">
      <button
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={onKeyDown}
        className={`md-select-trigger cf-press ${
          triggerClassName ??
          "md-type-body-medium flex h-10 w-full items-center gap-1.5 rounded-lg border border-[#e4e7ec] bg-[#fbfcfd] px-2.5 text-[#202124] outline-none transition hover:border-[#cfd5dd] hover:bg-white focus:border-[#bcc2cb] focus:bg-white focus:shadow-[0_0_0_3px_rgba(17,24,39,0.06)] disabled:cursor-not-allowed disabled:opacity-50"
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
      {open && createPortal(
        <div
          ref={menuRef}
          id={listboxId}
          role="listbox"
          aria-activedescendant={highlight >= 0 ? `${listboxId}-option-${highlight}` : undefined}
          data-direction={placement?.direction}
          className="md-select-menu cf-menu-in cf-dropdown fixed z-[1000] overflow-y-auto p-1"
          style={
            {
              top: placement?.top ?? 0,
              left: placement?.left ?? 0,
              minWidth: placement?.minWidth,
              maxWidth: placement?.maxWidth,
              maxHeight: placement?.maxHeight ?? 300,
              visibility: placement ? "visible" : "hidden",
            } satisfies CSSProperties
          }
        >
          {options.map((o, i) => {
            const active = o.value === value;
            const highlighted = i === highlight;
            return (
              <button
                key={o.value}
                id={`${listboxId}-option-${i}`}
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
                  <span className="md-type-body-medium block truncate">{o.label}</span>
                  {o.hint && <span className="md-type-label-small mt-0.5 block truncate text-[#8a9099]">{o.hint}</span>}
                </span>
                {active && <CheckIcon size={15} className="shrink-0 text-[#111827]" />}
              </button>
            );
          })}
        </div>
      , document.body)}
    </div>
  );
}

export default Select;
