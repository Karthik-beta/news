// A checkbox (design/components.md, "Forms"): the native box at 24px with its label beside it, so the label is
// part of the tap target; hint and error under the label.
import { controlProps, FieldError, FieldHint, FieldLabel, type FieldProps } from "../TextField/field";

export interface CheckboxProps extends FieldProps {
  checked?: boolean;
  /** Sent when checked; the browser sends "on" when left out. */
  value?: string;
}

export function Checkbox({ checked, value, ...field }: CheckboxProps) {
  return (
    <div className={`field checkbox${field.error ? " field--error" : ""}`}>
      <input
        className="checkbox__box"
        type="checkbox"
        defaultChecked={checked}
        value={value}
        {...controlProps(field)}
      />
      <div className="checkbox__text">
        <FieldLabel {...field} />
        <FieldHint {...field} />
        <FieldError {...field} />
      </div>
    </div>
  );
}
