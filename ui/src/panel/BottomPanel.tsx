/** The bottom panel: Problems, Index log, and References (architecture §11.5). Toggles with ⌘J. */

import { ChevronDown } from "lucide-react";
import { IconButton, PanelTabs, tabDomId } from "../components";
import { useEngineQuery } from "../ipc/queries";
import { ActionIds, actions, useShortcut } from "../keyboard";
import { useLayout, type PanelTab } from "../state/layout";
import { plural } from "../shell/status";
import "./BottomPanel.css";
import { IndexLogPanel } from "./IndexLogPanel";
import "./panel.css";
import { ProblemsPanel } from "./ProblemsPanel";
import { ReferencesPanel } from "./ReferencesPanel";

const LIST_ID = "panel";

export function BottomPanel() {
  const active = useLayout((state) => state.panelTab);
  const showPanelTab = useLayout((state) => state.showPanelTab);
  const problems = useEngineQuery("problems", {});
  const shortcut = useShortcut(ActionIds.togglePanel) ?? undefined;
  const count = problems.data?.length ?? 0;

  const tabs = [
    { id: "problems", label: "Problems", count, countLabel: plural(count, "problem") },
    { id: "index_log", label: "Index log" },
    { id: "references", label: "References" },
  ];

  return (
    <section className="panel" aria-label="Panel">
      <div className="panel-tabs">
        <PanelTabs
          id={LIST_ID}
          label="Panel views"
          tabs={tabs}
          activeId={active}
          panelId="panel-body"
          onSelect={(id) => {
            showPanelTab(id as PanelTab);
          }}
        />
        <IconButton
          icon={ChevronDown}
          label="Close Panel"
          {...(shortcut ? { shortcut } : {})}
          size="small"
          onClick={() => {
            actions.run(ActionIds.togglePanel);
          }}
        />
      </div>
      <div id="panel-body" role="tabpanel" className="panel-body" aria-labelledby={tabDomId(LIST_ID, active)}>
        {active === "problems" ? <ProblemsPanel /> : null}
        {active === "index_log" ? <IndexLogPanel /> : null}
        {active === "references" ? <ReferencesPanel /> : null}
      </div>
    </section>
  );
}
