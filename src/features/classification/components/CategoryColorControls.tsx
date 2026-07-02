import { PencilLine, RotateCcw, Trash2 } from "lucide-react";
import {
  isCustomCategory,
  type AppCategory,
  type CustomAppCategory,
} from "../../../shared/classification/categoryTokens";
import QuietColorField from "../../../shared/components/QuietColorField";
import QuietIconAction from "../../../shared/components/QuietIconAction";
import type { ColorDisplayFormat } from "../../../shared/lib/colorFormatting";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import { getCategoryManagementCopy } from "../classificationCategoryCopy.ts";

interface Props {
  categories: AppCategory[];
  colorFormat: ColorDisplayFormat;
  getCategoryLabel: (category: AppCategory) => string;
  getCategoryColor: (category: AppCategory) => string;
  onColorFormatChange: (nextFormat: ColorDisplayFormat) => void;
  onApplyColor: (category: AppCategory, color: string | null) => void;
  onRenameCategory: (category: CustomAppCategory) => void;
  onDeleteCategory: (category: CustomAppCategory) => void;
}

export default function CategoryColorControls({
  categories,
  colorFormat,
  getCategoryLabel,
  getCategoryColor,
  onColorFormatChange,
  onApplyColor,
  onRenameCategory,
  onDeleteCategory,
}: Props) {
  const categoryManagementCopy = getCategoryManagementCopy();
  if (categories.length === 0) {
    return null;
  }

  return (
    <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2">
      {categories.map((category) => {
        const label = getCategoryLabel(category);
        const color = getCategoryColor(category);

        return (
          <div
            key={category}
            className="rounded-[10px] border border-[var(--qp-border-subtle)] bg-[var(--qp-bg-panel)] px-3 py-2.5"
          >
            <div className="flex items-center justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <span
                  className="h-3.5 w-3.5 shrink-0 rounded-full border border-[var(--qp-bg-panel)]"
                  style={{ backgroundColor: color }}
                />
                <span className="truncate text-sm font-semibold text-[var(--qp-text-primary)]">{label}</span>
                {isCustomCategory(category) && (
                  <QuietIconAction
                    icon={<PencilLine size={13} />}
                    className="qp-icon-action-dimmed"
                    onClick={() => onRenameCategory(category)}
                    title={categoryManagementCopy.renameAction(label)}
                  />
                )}
              </div>

              <div className="flex shrink-0 items-center gap-1.5">
                <QuietColorField
                  color={color}
                  format={colorFormat}
                  onChange={(nextColor) => onApplyColor(category, nextColor)}
                  onFormatChange={onColorFormatChange}
                  title={UI_TEXT.mapping.color}
                />

                <QuietIconAction
                  icon={<RotateCcw size={13} />}
                  onClick={() => onApplyColor(category, null)}
                  title={UI_TEXT.mapping.restoreDefaultColor}
                />

                {isCustomCategory(category) && (
                  <QuietIconAction
                    icon={<Trash2 size={12} />}
                    tone="danger"
                    onClick={() => onDeleteCategory(category)}
                    title={UI_TEXT.mapping.deleteCategory(label)}
                  />
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
