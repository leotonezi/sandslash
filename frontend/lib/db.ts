import { Pool } from "pg";

export type DbPool = Pool;

let pool: Pool | null = null;

export function getPool(): Pool | null {
  const url = process.env.DATABASE_URL;
  if (!url) return null;

  if (!pool) {
    pool = new Pool({
      connectionString: url,
      max: 10,
      ssl: url.includes("sslmode=require") || process.env.PGSSL === "true"
        ? { rejectUnauthorized: false }
        : undefined,
    });
  }

  return pool;
}
