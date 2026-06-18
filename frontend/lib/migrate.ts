import fs from "fs";
import path from "path";
import type { DbPool } from "./db";

let migrated = false;

export async function ensureSchema(pool: DbPool): Promise<void> {
  if (migrated) return;
  const sql = fs.readFileSync(path.join(process.cwd(), "lib/schema.sql"), "utf-8");
  await pool.query(sql);
  migrated = true;
  console.log("[db] schema ensured");
}
