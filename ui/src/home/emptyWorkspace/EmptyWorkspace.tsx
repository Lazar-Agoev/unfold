/** An empty workspace: one line about Unfold and Add project, which opens the folder picker. */

import { Button } from "../../components";
import { addProject } from "../../state/actions";
import "./emptyWorkspace.css";

export function EmptyWorkspace() {
  return (
    <div className="home-empty">
      <div className="home-empty-block">
        <h1 className="home-empty-headline">Read a codebase by how it starts.</h1>
        <p className="home-empty-body">Add a folder. Unfold reads its TypeScript and Python, and never runs it.</p>
        <div className="home-empty-actions">
          <Button variant="primary" onClick={() => void addProject()}>
            Add project
          </Button>
        </div>
      </div>
    </div>
  );
}
