import * as vscode from 'vscode';
import { BlameProvider, BlameLineResult } from './blameProvider';

export class DecorationProvider {
    private aiDecorationType: vscode.TextEditorDecorationType;
    private aiModifiedDecorationType: vscode.TextEditorDecorationType;
    private humanDecorationType: vscode.TextEditorDecorationType;
    private disposables: vscode.Disposable[] = [];

    constructor(private blameProvider: BlameProvider) {
        // Create decoration types with gutter icons
        this.aiDecorationType = vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.createGutterIcon('#4CAF50'),
            gutterIconSize: 'contain',
            overviewRulerColor: '#4CAF50',
            overviewRulerLane: vscode.OverviewRulerLane.Left
        });

        this.aiModifiedDecorationType = vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.createGutterIcon('#FFC107'),
            gutterIconSize: 'contain',
            overviewRulerColor: '#FFC107',
            overviewRulerLane: vscode.OverviewRulerLane.Left
        });

        this.humanDecorationType = vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.createGutterIcon('#2196F3'),
            gutterIconSize: 'contain',
            overviewRulerColor: '#2196F3',
            overviewRulerLane: vscode.OverviewRulerLane.Left
        });
    }

    private createGutterIcon(color: string): vscode.Uri {
        // Create an SVG data URI for the gutter icon
        const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
            <circle cx="8" cy="8" r="4" fill="${color}"/>
        </svg>`;
        return vscode.Uri.parse(`data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`);
    }

    async updateDecorations(editor: vscode.TextEditor) {
        const config = vscode.workspace.getConfiguration('whogitit');

        if (!config.get('enabled', true) || !config.get('showGutterMarkers', true)) {
            // Clear all decorations
            editor.setDecorations(this.aiDecorationType, []);
            editor.setDecorations(this.aiModifiedDecorationType, []);
            editor.setDecorations(this.humanDecorationType, []);
            return;
        }

        const filePath = editor.document.uri.fsPath;
        const blame = await this.blameProvider.getBlame(filePath);

        if (!blame) {
            // No attribution data - clear decorations
            editor.setDecorations(this.aiDecorationType, []);
            editor.setDecorations(this.aiModifiedDecorationType, []);
            editor.setDecorations(this.humanDecorationType, []);
            return;
        }

        const aiRanges: vscode.DecorationOptions[] = [];
        const aiModifiedRanges: vscode.DecorationOptions[] = [];
        const humanRanges: vscode.DecorationOptions[] = [];

        for (const line of blame.lines) {
            const lineNumber = line.line - 1; // 0-indexed for VS Code
            if (lineNumber < 0 || lineNumber >= editor.document.lineCount) {
                continue;
            }

            const range = new vscode.Range(lineNumber, 0, lineNumber, 0);
            const decoration: vscode.DecorationOptions = {
                range,
                hoverMessage: this.createHoverMessage(line)
            };

            if (line.source.startsWith('AI {')) {
                aiRanges.push(decoration);
            } else if (line.source.startsWith('AIModified')) {
                aiModifiedRanges.push(decoration);
            } else if (line.source === 'Human') {
                humanRanges.push(decoration);
            }
            // Original lines don't get decorations
        }

        editor.setDecorations(this.aiDecorationType, aiRanges);
        editor.setDecorations(this.aiModifiedDecorationType, aiModifiedRanges);
        editor.setDecorations(this.humanDecorationType, humanRanges);
    }

    private createHoverMessage(line: BlameLineResult): vscode.MarkdownString {
        const md = new vscode.MarkdownString();
        md.isTrusted = true;

        let sourceLabel: string;
        let icon: string;

        if (line.source.startsWith('AI {')) {
            sourceLabel = 'AI Generated';
            icon = '🟢';
        } else if (line.source.startsWith('AIModified')) {
            sourceLabel = 'AI Modified by Human';
            icon = '🟡';
        } else if (line.source === 'Human') {
            sourceLabel = 'Human Written';
            icon = '🔵';
        } else {
            sourceLabel = 'Original';
            icon = '⚪';
        }

        md.appendMarkdown(`${icon} **${sourceLabel}**\n\n`);

        if (line.prompt_preview) {
            md.appendMarkdown(`**Prompt:** ${line.prompt_preview}\n\n`);
        }

        md.appendMarkdown(`*Commit:* \`${line.commit.substring(0, 7)}\` by ${line.author}`);

        return md;
    }

    dispose() {
        this.aiDecorationType.dispose();
        this.aiModifiedDecorationType.dispose();
        this.humanDecorationType.dispose();
        this.disposables.forEach(d => d.dispose());
    }
}
