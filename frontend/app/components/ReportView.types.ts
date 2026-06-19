export type Severity = "Critical" | "Warning" | "Info";

export type Category =
  | "Metadata"
  | "SocialTags"
  | "Structure"
  | "Links"
  | "Media"
  | "Crawlability"
  | "Security";

export interface Finding {
  check_id: string;
  category: Category;
  severity: Severity;
  message: string;
  penalty: number;
}

export interface PageReport {
  url: string;
  findings: Finding[];
  category_scores: Partial<Record<Category, number>>;
  score: number;
}

export interface AuditReport {
  root: string;
  pages: PageReport[];
  site_score: number;
  crawled_at: string;
}

export interface AuditRunSummary {
  id: number;
  host: string;
  root_url: string;
  site_score: number;
  crawled_at: string;
}

export interface RegressionResult {
  regressed: boolean;
  deltaPoints?: number;
}
