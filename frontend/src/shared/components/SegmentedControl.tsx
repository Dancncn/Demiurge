import type { CSSProperties, ReactNode } from "react";
import { CheckIcon } from "@/shared/components/Icons";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  hint?: string;
  icon?: ReactNode;
  disabled?: boolean;
}

interface Props<T extends string> {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (value: T) => void;
  ariaLabel: string;
  className?: string;
  showCheck?: boolean;
}

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
  className = "",
  showCheck = true,
}: Props<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={`md-segmented ${className}`}
      style={{ "--md-segment-count": options.length } as CSSProperties}
    >
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            disabled={option.disabled}
            className={`md-segment cf-press ${selected ? "is-selected" : ""}`}
            onClick={() => onChange(option.value)}
          >
            <span className="md-segment-label">
              {showCheck && selected ? <CheckIcon size={15} className="md-segment-check" /> : option.icon}
              <span>{option.label}</span>
            </span>
            {option.hint && <span className="md-segment-hint">{option.hint}</span>}
          </button>
        );
      })}
    </div>
  );
}

export default SegmentedControl;
