// KeyFigure's props, in their own module so KeyFigures can take a list of them. OpenUI tags this schema with the
// component's name when the library is built, so KeyFigures' signature reads `KeyFigures(children: KeyFigure[])`.
import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";

const valueRef = z.string().regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/);

export const keyFigureProps = z.object({
  label: z.string().min(1).describe("What the figure is, as text"),
  value: valueRef.describe("dp_… or dv_… reference"),
  change: valueRef.nullable().optional().describe("dv_… change, written in words"),
  against: z.string().regex(refPattern("pd")).nullable().optional().describe("pd_… the change is measured against"),
  trend: z.string().regex(refPattern("sr")).nullable().optional().describe("sr_… for a sparkline"),
});
