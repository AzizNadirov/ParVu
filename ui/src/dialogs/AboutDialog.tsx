import type { Ui } from "../lib/ui";
import { Dialog } from "../components/Dialog";
import { Markdown } from "./common";
import "./dialogs.css";

const REPO = "https://github.com/AzizNadirov/ParVu";
const DUCKDB_DOCS = "https://duckdb.org/docs/stable/sql/query_syntax/select";

export function aboutMarkdown(ui: Ui): string {
  const t = ui.t;
  return `# ${t("about.title")}

${t("dialog.about.description")}

## ${t("about.features")}

- ${t("about.feature.lazy")}
- ${t("about.feature.sql")}
- ${t("about.feature.autocomplete")}
- ${t("about.feature.pagination")}
- ${t("about.feature.editing")}
- ${t("about.feature.operations")}
- ${t("about.feature.export")}
- ${t("about.feature.themes")}

## ${t("about.quick_start")}

${t("about.step1")}
${t("about.step2")}
${t("about.step3")}
${t("about.step3_sub")}
${t("about.step4")}
${t("about.step5")}
${t("about.step6")}
${t("about.step7")}

## ${t("about.sql_reference")}

${t("about.sql_description")}

\`\`\`sql
-- View all data
SELECT * FROM data

-- Filter rows
SELECT * FROM data WHERE age > 25

-- Aggregate data
SELECT category, COUNT(*) as count
FROM data GROUP BY category

-- Multiple value filter
SELECT * FROM data WHERE status IN ('active', 'pending')
\`\`\`

${t("dialog.about.sql_docs")}
**[DuckDB SQL Query Syntax](${DUCKDB_DOCS})**

## ${t("about.shortcuts")}

- ${t("about.shortcuts.tab")}
- ${t("about.shortcuts.double")}
- ${t("about.shortcuts.right")}

## ${t("about.resources")}

- **GitHub**: [${REPO}](${REPO})
- **DuckDB**: [${DUCKDB_DOCS}](${DUCKDB_DOCS})
- **${t("dialog.about.report_issues")}**: [GitHub Issues](${REPO}/issues)

---

**${t("about.version")}**: ${ui.boot.version}  
**${t("about.built_with")}**: ${t("dialog.about.built_with_text")}  
**${t("about.license")}**: ${t("about.license_text")}
`;
}

function AboutDialog({ ui, close }: { ui: Ui; close: () => void }) {
  return (
    <Dialog
      title={ui.t("dialog.about")}
      onClose={close}
      className="pv-dialog-wide pv-dialog-tall"
      footer={
        <button className="primary" onClick={close}>
          {ui.t("btn.close")}
        </button>
      }
    >
      <Markdown source={aboutMarkdown(ui)} />
    </Dialog>
  );
}

export function openAboutDialog(ui: Ui): Promise<void> {
  return ui.showDialog<void>((close) => <AboutDialog ui={ui} close={() => close()} />, undefined);
}
