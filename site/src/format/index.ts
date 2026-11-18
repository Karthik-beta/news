// The shared formatters (design/content-style.md). Every component writes numbers, money, units, dates and
// periods through these, so "$391.0bn", "−5.7%" and "1 November 2024" look the same everywhere.
// Formatters only format: they never convert a value, a unit, a currency or a fiscal period.
export { currencyAffixes, formatMoney, type MoneyOptions, type MoneyStyle } from "./currency";
export {
  formatAxisMonth,
  formatDate,
  formatDateWithWeekday,
  formatMonthYear,
  formatPeriodLabel,
  formatShortDate,
  parseIsoDate,
} from "./dates";
export { DECIMAL_PATTERN, type Decimal, isDecimal } from "./decimal";
export {
  absDecimal,
  compactNumber,
  EN_DASH,
  formatAbout,
  formatCompact,
  formatNumber,
  formatRange,
  isNegative,
  isZeroValue,
  MINUS,
  SCALE_EXPONENT,
  type Scale,
  useTrueMinus,
  withSign,
} from "./number";
export {
  formatChange,
  formatPercent,
  formatPoints,
  formatQuantity,
  formatSigned,
  type UnitKind,
  unitKind,
} from "./units";
