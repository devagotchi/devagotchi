ALTER TABLE devagotchi.board_projects ADD COLUMN owner_id TEXT;
ALTER TABLE devagotchi.board_projects ADD COLUMN project_name TEXT;
CREATE UNIQUE INDEX board_projects_owner_name
  ON devagotchi.board_projects (tenant_id, owner_id, project_name);
