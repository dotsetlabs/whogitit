import * as vscode from 'vscode';
import { exec } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';

const execAsync = promisify(exec);

export interface BlameLineResult {
    line: number;
    commit: string;
    author: string;
    source: string;
    is_ai: boolean;
    is_human: boolean;
    prompt_index: number | null;
    prompt_preview: string | null;
    content: string;
}

export interface BlameSummary {
    total_lines: number;
    ai_lines: number;
    ai_modified_lines: number;
    human_lines: number;
    original_lines: number;
    ai_percentage: number;
}

export interface BlameResult {
    file: string;
    revision: string;
    lines: BlameLineResult[];
    summary: BlameSummary;
}

export class BlameProvider {
    private cache: Map<string, { result: BlameResult; timestamp: number }> = new Map();
    private readonly cacheTimeout = 30000; // 30 seconds

    async getBlame(filePath: string): Promise<BlameResult | null> {
        // Check cache
        const cached = this.cache.get(filePath);
        if (cached && Date.now() - cached.timestamp < this.cacheTimeout) {
            return cached.result;
        }

        try {
            const result = await this.fetchBlame(filePath);
            if (result) {
                this.cache.set(filePath, { result, timestamp: Date.now() });
            }
            return result;
        } catch (error) {
            console.error('Failed to fetch blame:', error);
            return null;
        }
    }

    private async fetchBlame(filePath: string): Promise<BlameResult | null> {
        const config = vscode.workspace.getConfiguration('whogitit');
        const whogititPath = config.get<string>('whogititPath', 'whogitit');

        // Get workspace folder for the file
        const workspaceFolder = vscode.workspace.getWorkspaceFolder(vscode.Uri.file(filePath));
        const cwd = workspaceFolder?.uri.fsPath || path.dirname(filePath);

        // Make path relative to workspace
        const relativePath = path.relative(cwd, filePath);

        try {
            const { stdout } = await execAsync(
                `${whogititPath} blame "${relativePath}" --format json`,
                { cwd, timeout: 10000 }
            );

            const result = JSON.parse(stdout);
            return result as BlameResult;
        } catch (error: any) {
            if (error.code === 'ENOENT') {
                console.log('whogitit binary not found');
            } else if (error.message?.includes('Not in a git repository')) {
                console.log('Not in a git repository');
            } else {
                console.error('whogitit error:', error.message);
            }
            return null;
        }
    }

    invalidateCache(filePath: string) {
        this.cache.delete(filePath);
    }

    clearCache() {
        this.cache.clear();
    }
}
