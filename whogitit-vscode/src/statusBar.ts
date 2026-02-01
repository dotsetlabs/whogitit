import * as vscode from 'vscode';
import { BlameProvider } from './blameProvider';

export class StatusBarProvider {
    private statusBarItem: vscode.StatusBarItem;

    constructor(private blameProvider: BlameProvider) {
        this.statusBarItem = vscode.window.createStatusBarItem(
            vscode.StatusBarAlignment.Right,
            100
        );
        this.statusBarItem.command = 'whogitit.showFileStats';
        this.statusBarItem.tooltip = 'Click to see detailed AI attribution stats';
    }

    async updateStatusBar(editor: vscode.TextEditor | undefined) {
        const config = vscode.workspace.getConfiguration('whogitit');

        if (!config.get('enabled', true) || !config.get('showStatusBar', true)) {
            this.statusBarItem.hide();
            return;
        }

        if (!editor) {
            this.statusBarItem.hide();
            return;
        }

        const blame = await this.blameProvider.getBlame(editor.document.uri.fsPath);

        if (!blame) {
            this.statusBarItem.hide();
            return;
        }

        const aiPercentage = blame.summary.ai_percentage;

        // Determine icon based on percentage
        let icon: string;
        if (aiPercentage >= 80) {
            icon = '$(robot)$(robot)$(robot)';
        } else if (aiPercentage >= 50) {
            icon = '$(robot)$(robot)';
        } else if (aiPercentage >= 20) {
            icon = '$(robot)';
        } else if (aiPercentage > 0) {
            icon = '$(person)';
        } else {
            icon = '$(person)';
        }

        this.statusBarItem.text = `${icon} ${aiPercentage.toFixed(0)}% AI`;
        this.statusBarItem.tooltip = new vscode.MarkdownString(
            `**AI Attribution**\n\n` +
            `- AI Generated: ${blame.summary.ai_lines} lines\n` +
            `- AI Modified: ${blame.summary.ai_modified_lines} lines\n` +
            `- Human Written: ${blame.summary.human_lines} lines\n` +
            `- Original: ${blame.summary.original_lines} lines\n\n` +
            `Click to see detailed stats`
        );

        this.statusBarItem.show();
    }

    dispose() {
        this.statusBarItem.dispose();
    }
}
