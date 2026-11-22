// A multi-line text field (design/components.md, "Forms"): the TextField look, resizable in height only.
import { controlProps, Field, type FieldProps } from "../TextField/field";

export interface TextAreaProps extends FieldProps {
  value?: string;
  rows?: number;
}

export function TextArea({ value, rows = 5, ...field }: TextAreaProps) {
  return (
    <Field field={field}>
      <textarea className="field__control text-area" rows={rows} defaultValue={value} {...controlProps(field)} />
    </Field>
  );
}
