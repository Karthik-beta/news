// A drop-down choice (design/components.md, "Forms"): the native <select> in the field look, so the phone's own
// picker opens. `value` is the option chosen when the page was rendered.
import { controlProps, Field, type FieldProps } from "../TextField/field";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends FieldProps {
  options: readonly SelectOption[];
  value?: string;
}

export function Select({ options, value, ...field }: SelectProps) {
  return (
    <Field field={field}>
      <select className="field__control select" defaultValue={value} {...controlProps(field)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </Field>
  );
}
