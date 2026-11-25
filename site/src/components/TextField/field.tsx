// The parts every form field shares (design/components.md, "Forms"; design/accessibility.md, "Forms"): a visible
// label above, an optional hint under it, the control, and the error under the control with an icon and words.
// The control is described by the hint and the error, and marked invalid while it has an error.
import type { ReactNode } from "react";
import { Icon } from "../Icon/Icon";
import { VisuallyHidden } from "../VisuallyHidden/VisuallyHidden";

export interface FieldProps {
  /** Unique on the page; the error summary links to it. */
  id: string;
  /** The form field name; the id when left out. */
  name?: string;
  label: string;
  hint?: string;
  /** The error message; the field shows its error state while this is set. */
  error?: string;
  /** true: "(required)" after the label and the required attribute. false: "(optional)". Left out: no marker. */
  required?: boolean;
}

/** The control's own attributes: id, name, required, invalid and what describes it. */
export function controlProps({ id, name, hint, error, required }: FieldProps) {
  const describedBy = [hint ? `${id}-hint` : "", error ? `${id}-error` : ""].filter(Boolean).join(" ");
  return {
    id,
    name: name ?? id,
    required: required === true || undefined,
    "aria-invalid": error ? true : undefined,
    "aria-describedby": describedBy || undefined,
  };
}

export function FieldLabel({ id, label, required }: FieldProps) {
  return (
    <label className="field__label" htmlFor={id}>
      {label}
      {required !== undefined && (
        <>
          {" "}
          <span className="field__marker">({required ? "required" : "optional"})</span>
        </>
      )}
    </label>
  );
}

export function FieldHint({ id, hint }: FieldProps) {
  return hint ? (
    <p className="field__hint" id={`${id}-hint`}>
      {hint}
    </p>
  ) : null;
}

export function FieldError({ id, error }: FieldProps) {
  return error ? (
    <p className="field__error" id={`${id}-error`}>
      <Icon name="error" className="field__error-icon" />
      <VisuallyHidden>Error: </VisuallyHidden>
      {error}
    </p>
  ) : null;
}

/** Label, hint, control, error: the stacked layout of text fields, text areas and selects. */
export function Field({ field, children }: { field: FieldProps; children: ReactNode }) {
  return (
    <div className={`field${field.error ? " field--error" : ""}`}>
      <FieldLabel {...field} />
      <FieldHint {...field} />
      {children}
      <FieldError {...field} />
    </div>
  );
}
