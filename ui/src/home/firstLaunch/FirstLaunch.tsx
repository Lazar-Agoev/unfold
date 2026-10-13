/**
 * First launch: an empty workspace. Add project… opens the folder picker; "Opened before" lists the
 * folders removed from the workspace, with when (they were open until removed), each added back with
 * `readd_project`.
 */

import { Button, TextButton, showToast } from "../../components";
import { Icon } from "../../icons";
import { toUnfoldError, type RemovedProject } from "../../ipc";
import { displayPath, relativeTime } from "../../lib/format";
import { addProject } from "../../state/actions";
import { setHomeView } from "../../state/layout";
import { showHome } from "../../state/shownBoard";
import { readdProject } from "../../state/workspace";
import "./firstLaunch.css";

async function addBack(folder: RemovedProject): Promise<void> {
  try {
    await readdProject(folder.path);
    setHomeView("overview");
    showHome();
  } catch (error) {
    showToast({ title: `Couldn't add ${folder.name}`, body: toUnfoldError(error).message, tone: "danger" });
  }
}

export function FirstLaunch({ removed }: { readonly removed: readonly RemovedProject[] }) {
  return (
    <div className="home-first">
      <div className="home-first-block">
        <div className="home-first-intro">
          <p className="home-caps">Unfold</p>
          <h1 className="home-first-headline">Read a codebase by how it starts.</h1>
          <p className="home-first-body">
            Connect a repository. Unfold reads TypeScript and Python, finds the ways the code starts, and lets you follow each call on a
            board. It never runs your code.
          </p>
        </div>
        <div className="home-first-actions">
          <Button variant="primary" onClick={() => void addProject()}>
            Add project…
          </Button>
        </div>
        {removed.length > 0 ? (
          <div className="home-first-before">
            <h2 className="home-caps home-first-before-label">Opened before</h2>
            <ul className="home-first-before-list">
              {removed.map((folder) => (
                <li key={folder.path} className="home-first-before-row">
                  <Icon name="folder" className="home-first-before-icon" />
                  {/* The folder's name tells the rows apart, so a long path loses its start. */}
                  <span className="home-first-before-path" title={folder.path}>
                    <bdi>{displayPath(folder.path)}</bdi>
                  </span>
                  <span className="home-first-before-when">{`last opened ${relativeTime(folder.removedAtMs)}`}</span>
                  <span className="home-first-before-action">
                    <TextButton aria-label={`Add ${folder.name}`} onClick={() => void addBack(folder)}>
                      Add
                    </TextButton>
                  </span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </div>
  );
}
