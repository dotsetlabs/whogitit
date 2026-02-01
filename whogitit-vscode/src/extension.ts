import * as vscode from 'vscode';
import { BlameProvider } from './blameProvider';
import { DecorationProvider } from './decorations';
import { HoverProvider } from './hoverProvider';
import { CodeLensProvider } from './codeLensProvider';
import { StatusBarProvider } from './statusBar';

let blameProvider: BlameProvider;
let decorationProvider: DecorationProvider;
let hoverProviderDisposable: vscode.Disposable | undefined;
let codeLensProviderDisposable: vscode.Disposable | undefined;
let statusBarProvider: StatusBarProvider;

export function activate(context: vscode.ExtensionContext) {
    console.log('whogitit extension is now active');

    // Initialize providers
    blameProvider = new BlameProvider();
    decorationProvider = new DecorationProvider(blameProvider);
    statusBarProvider = new StatusBarProvider(blameProvider);

    // Register hover provider
    const config = vscode.workspace.getConfiguration('whogitit');
    if (config.get('showHoverTooltips', true)) {
        hoverProviderDisposable = vscode.languages.registerHoverProvider(
            { scheme: 'file' },
            new HoverProvider(blameProvider)
        );
        context.subscriptions.push(hoverProviderDisposable);
    }

    // Register CodeLens provider
    if (config.get('showCodeLens', true)) {
        codeLensProviderDisposable = vscode.languages.registerCodeLensProvider(
            { scheme: 'file' },
            new CodeLensProvider(blameProvider)
        );
        context.subscriptions.push(codeLensProviderDisposable);
    }

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('whogitit.showPrompt', () => showPromptCommand()),
        vscode.commands.registerCommand('whogitit.toggleDecorations', () => toggleDecorationsCommand()),
        vscode.commands.registerCommand('whogitit.refreshBlame', () => refreshBlameCommand()),
        vscode.commands.registerCommand('whogitit.showFileStats', () => showFileStatsCommand())
    );

    // Listen for active editor changes
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(editor => {
            if (editor) {
                decorationProvider.updateDecorations(editor);
                statusBarProvider.updateStatusBar(editor);
            }
        })
    );

    // Listen for document changes
    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument(event => {
            const editor = vscode.window.activeTextEditor;
            if (editor && event.document === editor.document) {
                // Invalidate cache for this file and refresh
                blameProvider.invalidateCache(event.document.uri.fsPath);
                decorationProvider.updateDecorations(editor);
                statusBarProvider.updateStatusBar(editor);
            }
        })
    );

    // Listen for configuration changes
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(event => {
            if (event.affectsConfiguration('whogitit')) {
                // Refresh all decorations
                const editor = vscode.window.activeTextEditor;
                if (editor) {
                    decorationProvider.updateDecorations(editor);
                    statusBarProvider.updateStatusBar(editor);
                }
            }
        })
    );

    // Initial update for active editor
    if (vscode.window.activeTextEditor) {
        decorationProvider.updateDecorations(vscode.window.activeTextEditor);
        statusBarProvider.updateStatusBar(vscode.window.activeTextEditor);
    }
}

export function deactivate() {
    decorationProvider?.dispose();
    statusBarProvider?.dispose();
}

async function showPromptCommand() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showInformationMessage('No active editor');
        return;
    }

    const line = editor.selection.active.line + 1; // 1-indexed
    const filePath = editor.document.uri.fsPath;

    try {
        const blame = await blameProvider.getBlame(filePath);
        if (!blame) {
            vscode.window.showInformationMessage('No attribution data available for this file');
            return;
        }

        const lineResult = blame.lines.find(l => l.line === line);
        if (!lineResult) {
            vscode.window.showInformationMessage('No attribution data for this line');
            return;
        }

        if (!lineResult.is_ai) {
            vscode.window.showInformationMessage('This line is not AI-generated');
            return;
        }

        if (lineResult.prompt_preview) {
            // Show in a panel
            const panel = vscode.window.createWebviewPanel(
                'whogititPrompt',
                `AI Prompt - Line ${line}`,
                vscode.ViewColumn.Beside,
                {}
            );

            panel.webview.html = `
                <!DOCTYPE html>
                <html>
                <head>
                    <style>
                        body { font-family: var(--vscode-font-family); padding: 20px; }
                        .prompt { background: var(--vscode-textBlockQuote-background);
                                  padding: 15px; border-radius: 4px; white-space: pre-wrap; }
                        .meta { color: var(--vscode-descriptionForeground); margin-top: 10px; }
                    </style>
                </head>
                <body>
                    <h2>AI Prompt</h2>
                    <div class="prompt">${escapeHtml(lineResult.prompt_preview)}</div>
                    <div class="meta">
                        <p>Source: ${lineResult.source}</p>
                        <p>Commit: ${lineResult.commit}</p>
                    </div>
                </body>
                </html>
            `;
        } else {
            vscode.window.showInformationMessage('Prompt text not available for this line');
        }
    } catch (error) {
        vscode.window.showErrorMessage(`Failed to get prompt: ${error}`);
    }
}

function toggleDecorationsCommand() {
    const config = vscode.workspace.getConfiguration('whogitit');
    const currentValue = config.get('enabled', true);
    config.update('enabled', !currentValue, vscode.ConfigurationTarget.Global);
    vscode.window.showInformationMessage(
        `whogitit decorations ${!currentValue ? 'enabled' : 'disabled'}`
    );
}

async function refreshBlameCommand() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        return;
    }

    blameProvider.invalidateCache(editor.document.uri.fsPath);
    await decorationProvider.updateDecorations(editor);
    statusBarProvider.updateStatusBar(editor);
    vscode.window.showInformationMessage('AI attribution refreshed');
}

async function showFileStatsCommand() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showInformationMessage('No active editor');
        return;
    }

    try {
        const blame = await blameProvider.getBlame(editor.document.uri.fsPath);
        if (!blame) {
            vscode.window.showInformationMessage('No attribution data available for this file');
            return;
        }

        const aiLines = blame.summary.ai_lines;
        const aiModLines = blame.summary.ai_modified_lines;
        const humanLines = blame.summary.human_lines;
        const originalLines = blame.summary.original_lines;
        const totalLines = blame.summary.total_lines;
        const aiPercentage = blame.summary.ai_percentage;

        const message = [
            `AI Attribution for ${blame.file}`,
            ``,
            `Total Lines: ${totalLines}`,
            `AI Generated: ${aiLines} (${((aiLines / totalLines) * 100).toFixed(1)}%)`,
            `AI Modified: ${aiModLines} (${((aiModLines / totalLines) * 100).toFixed(1)}%)`,
            `Human Written: ${humanLines} (${((humanLines / totalLines) * 100).toFixed(1)}%)`,
            `Original: ${originalLines} (${((originalLines / totalLines) * 100).toFixed(1)}%)`,
            ``,
            `AI Involvement: ${aiPercentage.toFixed(1)}%`
        ].join('\n');

        vscode.window.showInformationMessage(message, { modal: true });
    } catch (error) {
        vscode.window.showErrorMessage(`Failed to get file stats: ${error}`);
    }
}

function escapeHtml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
}
