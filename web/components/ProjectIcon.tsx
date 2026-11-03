import type { Project } from "@/lib/api";
import styles from "./ProjectIcon.module.scss";

/** A project's avatar: its uploaded icon, its emoji, or its initials. */
export function ProjectIcon({
  project,
  size = 32,
  className,
}: {
  project: Pick<Project, "name" | "identifier" | "icon_url" | "icon_emoji">;
  size?: number;
  className?: string;
}) {
  const style = { width: size, height: size, fontSize: size * 0.5 };
  return (
    <span className={`${styles.icon} ${className ?? ""}`} style={style} aria-hidden>
      {project.icon_url ? (
        <img src={project.icon_url} alt="" />
      ) : project.icon_emoji ? (
        project.icon_emoji
      ) : (
        <span className={styles.letters}>{project.identifier.slice(0, 2)}</span>
      )}
    </span>
  );
}
