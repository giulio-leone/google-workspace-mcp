import { chromium } from 'playwright';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

export const NOTEBOOKLM_HOME = path.join(os.homedir(), '.notebooklm');
export const STORAGE_PATH = path.join(NOTEBOOKLM_HOME, 'storage_state.json');

export async function login() {
  if (!fs.existsSync(NOTEBOOKLM_HOME)) {
    fs.mkdirSync(NOTEBOOKLM_HOME, { recursive: true });
  }

  console.log('Opening browser for Google login. Please sign in...');
  
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();

  await page.goto('https://notebooklm.google.com/');

  console.log('Waiting for login to complete (looking for user interaction or redirect)...');

  // Wait until we are successfully on the notebooklm page and not on a login page
  try {
    await page.waitForFunction(() => {
      const url = window.location.href;
      return url.includes('notebooklm.google.com') && !url.includes('accounts.google.com');
    }, { timeout: 300000 }); // 5 minutes timeout for manual login
  } catch (err) {
    console.error('Login timed out or failed.', err);
    await browser.close();
    throw err;
  }

  // Save the cookies
  const state = await context.storageState();
  fs.writeFileSync(STORAGE_PATH, JSON.stringify(state, null, 2));

  console.log(`Saved authentication state to ${STORAGE_PATH}`);
  await browser.close();
}

export function getCookies(): string {
  if (!fs.existsSync(STORAGE_PATH)) {
    throw new Error('No storage state found. Please run authenticate operation first.');
  }

  const state = JSON.parse(fs.readFileSync(STORAGE_PATH, 'utf-8'));
  const cookies = state.cookies || [];
  
  // Filter and prioritize .google.com cookies over regional
  const filtered: Record<string, string> = {};
  for (const c of cookies) {
    const domain = c.domain || '';
    if (domain.includes('google.com') || domain.includes('.google.')) {
      if (!filtered[c.name] || domain === '.google.com') {
        filtered[c.name] = c.value;
      }
    }
  }

  if (!filtered['SID']) {
    throw new Error('SID cookie not found. Authentication might have failed or expired.');
  }

  return Object.entries(filtered).map(([k, v]) => `${k}=${v}`).join('; ');
}

export async function fetchTokens(): Promise<{ csrfToken: string; sessionId: string; cookieHeader: string }> {
  const cookieHeader = getCookies();
  
  const response = await fetch('https://notebooklm.google.com/', {
    headers: {
      'Cookie': cookieHeader,
      'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
    }
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch notebooklm homepage: ${response.status} ${response.statusText}`);
  }

  const html = await response.text();

  // Extract SNlM0e
  const csrfMatch = html.match(/"SNlM0e"\s*:\s*"([^"]+)"/);
  if (!csrfMatch) {
    throw new Error('Could not find CSRF token (SNlM0e) in page HTML. Authentication might have expired.');
  }
  
  // Extract FdrFJe
  const sidMatch = html.match(/"FdrFJe"\s*:\s*"([^"]+)"/);
  if (!sidMatch) {
    throw new Error('Could not find Session ID (FdrFJe) in page HTML. Authentication might have expired.');
  }

  return {
    csrfToken: csrfMatch[1],
    sessionId: sidMatch[1],
    cookieHeader
  };
}
