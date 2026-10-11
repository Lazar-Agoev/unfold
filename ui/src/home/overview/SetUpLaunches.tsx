/**
 * How it starts for a project without launches: why that matters here, then launches to start from,
 * from the deploy files (`render.yaml`) and from FastAPI apps no launch reaches. Each opens the launch
 * editor prefilled; nothing is saved until it is reviewed there.
 */

import { Button, TextButton } from "../../components";
import { Icon } from "../../icons";
import { useEngineQuery, type DeployImport, type ProjectId, type SuggestedApp } from "../../ipc";
import { plural } from "../../lib/format";
import { openDialog } from "../../state/overlays";
import { deployBreakdown, setupSentence } from "./words";

/** Deploy-file launches listed before "and N more". */
const LISTED = 4;

function addLaunch(project: ProjectId): void {
  openDialog({ kind: "launchEditor", project, mode: "add" });
}

function FromDeployFiles({ project, deploy }: { readonly project: ProjectId; readonly deploy: DeployImport }) {
  const { launches } = deploy;
  const more = launches.length - LISTED;
  return (
    <section className="home-setup-panel home-setup-deploy" aria-label="From your deploy files">
      <h4 className="home-setup-eyebrow">From your deploy files</h4>
      <div className="home-setup-lead">
        <p className="home-setup-lead-title">{plural(launches.length, "start command")}</p>
        <p className="home-setup-lead-text">{deployBreakdown(deploy)}</p>
      </div>
      <ul className="home-setup-services">
        {launches.slice(0, LISTED).map((launch) => (
          <li key={`${launch.renderFile}:${launch.service}`} className="home-setup-service">
            <span className="home-setup-service-type">{launch.serviceType.toUpperCase()}</span>
            <span className="home-setup-service-name">{launch.service}</span>
            {launch.schedule === null ? null : <span className="home-setup-service-schedule">{launch.schedule}</span>}
          </li>
        ))}
        {more > 0 ? (
          <li className="home-setup-service">
            <span className="home-setup-service-type" />
            <span className="home-setup-service-more">and {more.toLocaleString("en-US")} more</span>
          </li>
        ) : null}
      </ul>
      <div className="home-setup-review">
        <Button
          variant="primary"
          onClick={() => {
            openDialog({ kind: "launchEditor", project, mode: "review", launches });
          }}
        >
          Review {plural(launches.length, "launch", "launches")}…
        </Button>
        <span className="home-setup-review-note">Names, schedules, and comments come along</span>
      </div>
    </section>
  );
}

function FromYourCode({ project, name, apps }: { readonly project: ProjectId; readonly name: string; readonly apps: readonly SuggestedApp[] }) {
  return (
    <section className="home-setup-panel home-setup-code" aria-label="From your code">
      <div className="home-setup-code-head">
        <h4 className="home-setup-eyebrow">From your code</h4>
        <p className="home-setup-lead-text">
          {plural(apps.length, "FastAPI app")} that no launch reaches. Each suggestion opens prefilled; check it against how you deploy.
        </p>
      </div>
      <ul className="home-setup-apps">
        {apps.map((app) => (
          <li key={`${app.path}:${app.attribute}`} className="home-setup-app">
            <div className="home-setup-app-text">
              <div className="home-setup-app-path">
                <span className="home-setup-app-file">{app.path}</span>
                <span className="home-setup-app-attribute">{app.attribute}</span>
              </div>
              <div className="home-setup-app-command">
                {app.folder === "." ? name : app.folder} $ {app.command}
              </div>
            </div>
            <span className="home-setup-app-routes">{plural(app.routes, "route")}</span>
            <Button
              variant="secondary"
              size="sm"
              aria-label={`Add launch for ${app.path}`}
              onClick={() => {
                openDialog({ kind: "launchEditor", project, mode: "add", prefill: { command: app.command, cwd: app.folder } });
              }}
            >
              Add launch
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function SetUpLaunches({ project, name }: { readonly project: ProjectId; readonly name: string }) {
  const suggestions = useEngineQuery("launch_suggestions", { project });
  const deploy = useEngineQuery("import_deploy_files", { project });
  const deployFiles = deploy.data && deploy.data.launches.length > 0 ? deploy.data : null;
  const apps = suggestions.data?.apps ?? [];
  return (
    <section className="home-setup" aria-label={`Tell Unfold how ${name} starts`}>
      <div className="home-setup-intro">
        <h3 className="home-setup-title">Tell Unfold how {name} starts</h3>
        {suggestions.isPending ? null : (
          <p className="home-setup-text">{setupSentence(suggestions.data)} Give Unfold the commands you deploy with. It reads them and never runs them.</p>
        )}
      </div>
      {deployFiles || apps.length > 0 ? (
        <div className="home-setup-panels">
          {deployFiles ? <FromDeployFiles project={project} deploy={deployFiles} /> : null}
          {apps.length > 0 ? <FromYourCode project={project} name={name} apps={apps} /> : null}
        </div>
      ) : null}
      <div className="home-setup-footer">
        <TextButton
          className="home-setup-by-hand"
          onClick={() => {
            addLaunch(project);
          }}
        >
          <Icon name="plus" size={14} />
          Add a launch by hand
        </TextButton>
        <span className="home-setup-footer-note">Launches are saved to unfold.toml, so your team gets them too</span>
      </div>
    </section>
  );
}
