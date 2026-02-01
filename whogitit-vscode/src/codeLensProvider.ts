import * as vscode from 'vscode';
import { BlameProvider, BlameLineResult } from './blameProvider';

interface AIRegion {
    startLine: number;
    endLine: number;
    lineCount: number;
    hasPrompt: boolean;
}

export class CodeLensProvider implements vscode.CodeLensProvider {
    private _onDidChangeCodeLenses = new vscode.EventEmitter<void>();
    public readonly onDidChangeCodeLenses = this._onDidChangeCodeLenses.event;

    constructor(private blameProvider: BlameProvider) {}

    async provideCodeLenses(
        document: vscode.TextDocument,
        _token: vscode.CancellationToken
    ): Promise<vscode.CodeLens[]> {
        const config = vscode.workspace.getConfiguration('whogitit');

        if (!config.get('enabled', true) || !config.get('showCodeLens', true)) {
            return [];
        }

        const blame = await this.blameProvider.getBlame(document.uri.fsPath);
        if (!blame) {
            return [];
        }

        // Find contiguous AI regions
        const regions = this.findAIRegions(blame.lines);

        // Create CodeLens for each region
        return regions.map(region => {
            const range = new vscode.Range(region.startLine - 1, 0, region.startLine - 1, 0);

            const title = region.hasPrompt
                ? `🤖 AI: ${region.lineCount} lines | View prompt`
                : `🤖 AI: ${region.lineCount} lines`;

            return new vscode.CodeLens(range, {
                title,
                command: region.hasPrompt ? 'whogitit.showPrompt' : undefined,
                tooltip: `${region.lineCount} AI-generated lines (${region.startLine}-${region.endLine})`
            });
        });
    }

    private findAIRegions(lines: BlameLineResult[]): AIRegion[] {
        const regions: AIRegion[] = [];
        let currentRegion: AIRegion | null = null;

        // Sort lines by line number
        const sortedLines = [...lines].sort((a, b) => a.line - b.line);

        for (const line of sortedLines) {
            const isAI = line.source.startsWith('AI {') || line.source.startsWith('AIModified');

            if (isAI) {
                if (currentRegion && line.line === currentRegion.endLine + 1) {
                    // Extend current region
                    currentRegion.endLine = line.line;
                    currentRegion.lineCount++;
                    if (line.prompt_preview) {
                        currentRegion.hasPrompt = true;
                    }
                } else {
                    // Start new region
                    if (currentRegion && currentRegion.lineCount >= 3) {
                        regions.push(currentRegion);
                    }
                    currentRegion = {
                        startLine: line.line,
                        endLine: line.line,
                        lineCount: 1,
                        hasPrompt: !!line.prompt_preview
                    };
                }
            } else {
                // Non-AI line - close current region
                if (currentRegion && currentRegion.lineCount >= 3) {
                    regions.push(currentRegion);
                }
                currentRegion = null;
            }
        }

        // Don't forget the last region
        if (currentRegion && currentRegion.lineCount >= 3) {
            regions.push(currentRegion);
        }

        return regions;
    }

    refresh() {
        this._onDidChangeCodeLenses.fire();
    }
}
