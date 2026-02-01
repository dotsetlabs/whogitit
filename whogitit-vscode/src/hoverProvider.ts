import * as vscode from 'vscode';
import { BlameProvider } from './blameProvider';

export class HoverProvider implements vscode.HoverProvider {
    constructor(private blameProvider: BlameProvider) {}

    async provideHover(
        document: vscode.TextDocument,
        position: vscode.Position,
        _token: vscode.CancellationToken
    ): Promise<vscode.Hover | null> {
        const config = vscode.workspace.getConfiguration('whogitit');

        if (!config.get('enabled', true) || !config.get('showHoverTooltips', true)) {
            return null;
        }

        const blame = await this.blameProvider.getBlame(document.uri.fsPath);
        if (!blame) {
            return null;
        }

        const lineNumber = position.line + 1; // 1-indexed
        const lineResult = blame.lines.find(l => l.line === lineNumber);

        if (!lineResult || !lineResult.is_ai) {
            return null; // Only show hover for AI lines
        }

        const md = new vscode.MarkdownString();
        md.isTrusted = true;
        md.supportHtml = true;

        // Header with source type
        let sourceLabel: string;
        let sourceIcon: string;

        if (lineResult.source.startsWith('AI {')) {
            sourceLabel = 'AI Generated';
            sourceIcon = '🟢';
        } else if (lineResult.source.startsWith('AIModified')) {
            sourceLabel = 'AI Modified by Human';
            sourceIcon = '🟡';
        } else {
            sourceLabel = 'AI';
            sourceIcon = '🤖';
        }

        md.appendMarkdown(`### ${sourceIcon} ${sourceLabel}\n\n`);

        // Prompt preview
        if (lineResult.prompt_preview) {
            md.appendMarkdown(`**Prompt:**\n\n`);
            md.appendCodeblock(lineResult.prompt_preview, 'text');
            md.appendMarkdown('\n');
        }

        // Metadata
        md.appendMarkdown(`---\n\n`);
        md.appendMarkdown(`**Commit:** \`${lineResult.commit.substring(0, 7)}\`\n\n`);
        md.appendMarkdown(`**Author:** ${lineResult.author}\n\n`);

        // Command link to show full prompt
        if (lineResult.prompt_preview) {
            md.appendMarkdown(`[View Full Prompt](command:whogitit.showPrompt)`);
        }

        return new vscode.Hover(md, document.lineAt(position.line).range);
    }
}
