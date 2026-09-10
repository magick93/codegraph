import { createClient } from '@supabase/supabase-js';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const SUPABASE_URL = process.env.SUPABASE_URL ?? 'http://localhost:54321';
const SUPABASE_SERVICE_ROLE_KEY = process.env.SUPABASE_SERVICE_ROLE_KEY ?? '';

const AUTH_DIR = path.resolve(__dirname);
const CONTEXT_FILE = path.join(AUTH_DIR, 'org-context.json');

async function globalTeardown() {
  if (!fs.existsSync(CONTEXT_FILE)) {
    console.log('[teardown] No org-context.json found, skipping cleanup.');
    return;
  }

  const supabaseAdmin = createClient(SUPABASE_URL, SUPABASE_SERVICE_ROLE_KEY, {
    auth: { autoRefreshToken: false, persistSession: false },
  });

  const context = JSON.parse(fs.readFileSync(CONTEXT_FILE, 'utf-8'));
  const userIds = [
    context.acme.ownerId,
    context.acme.employeeId,
    context.acme.managerId,
    context.highfive.ownerId,
  ];

  console.log('[teardown] Deleting test users...');
  for (const userId of userIds) {
    const { error } = await supabaseAdmin.auth.admin.deleteUser(userId);
    if (error) console.warn(`[teardown] Failed to delete user ${userId}: ${error.message}`);
  }

  // Cleanup all worker-indexed storage state files
  const entries = fs.readdirSync(AUTH_DIR);
  for (const entry of entries) {
    if (entry.endsWith('.storageState.json')) {
      fs.unlinkSync(path.join(AUTH_DIR, entry));
    }
  }

  if (fs.existsSync(CONTEXT_FILE)) fs.unlinkSync(CONTEXT_FILE);

  console.log('[teardown] Cleanup complete.');
}

export default globalTeardown;
