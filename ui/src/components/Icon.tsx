// Material icons (from @mdi/js, imported one by one so only these end up in
// the bundle). Drawn in `currentColor`, so they follow the text colour.
import {
  mdiArrowCollapseDown,
  mdiArrowDown,
  mdiArrowUp,
  mdiCalculatorVariantOutline,
  mdiChartBar,
  mdiCheck,
  mdiChevronDown,
  mdiChevronLeft,
  mdiChevronRight,
  mdiClose,
  mdiFilterVariant,
  mdiFolderOpenOutline,
  mdiInformationOutline,
  mdiPageFirst,
  mdiPageLast,
  mdiPlay,
  mdiPlus,
  mdiRestore,
  mdiSigma,
  mdiTableMergeCells,
  mdiUndo,
} from "@mdi/js";

export const icons = {
  append: mdiArrowCollapseDown,
  arrowDown: mdiArrowDown,
  arrowUp: mdiArrowUp,
  chart: mdiChartBar,
  check: mdiCheck,
  chevronDown: mdiChevronDown,
  chevronLeft: mdiChevronLeft,
  chevronRight: mdiChevronRight,
  close: mdiClose,
  filter: mdiFilterVariant,
  first: mdiPageFirst,
  groupBy: mdiSigma,
  info: mdiInformationOutline,
  join: mdiTableMergeCells,
  last: mdiPageLast,
  math: mdiCalculatorVariantOutline,
  open: mdiFolderOpenOutline,
  plus: mdiPlus,
  reset: mdiRestore,
  run: mdiPlay,
  undo: mdiUndo,
} as const;

export type IconName = keyof typeof icons;

export function Icon({ name, size = 18, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg className={`pv-icon${className ? ` ${className}` : ""}`} viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
      <path d={icons[name]} fill="currentColor" />
    </svg>
  );
}
