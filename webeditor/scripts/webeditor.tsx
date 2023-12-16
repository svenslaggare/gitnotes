import React from "react";
import ReactDOM from 'react-dom'

import AceEditor from "react-ace";
import "ace-builds/src-noconflict/mode-markdown";
import "ace-builds/src-noconflict/theme-textmate";

import "brace";
import "brace/ext/searchbox";

import ReactMarkdown from "react-markdown";
import {Prism as SyntaxHighlighter} from 'react-syntax-highlighter'

import axios from "axios";

class WebEditorMainProps {
    filePath: string;
    isReadOnly: boolean;
    isStandalone: boolean;
}

interface WebEditorMainState {
    content: string;
    isReadOnly: boolean;
    isStandalone: boolean;
    showText: boolean;
    showRendered: boolean;

    success: string;
    error: string;
    snippetOutput: string;
    snippetOutputContent: string;
}

class WebEditorMain extends React.Component<WebEditorMainProps, WebEditorMainState> {
    editArea: React.RefObject<any>;

    constructor(props) {
        super(props);

        this.state = {
            content: "",
            isReadOnly: this.props.isReadOnly,
            isStandalone: this.props.isStandalone,
            showText: !this.props.isReadOnly,
            showRendered: true,
            success: null,
            error: null,
            snippetOutput: null,
            snippetOutputContent: null
        };

        this.editArea = React.createRef();
        this.fetchContent();
    }

    render() {
        return (
            <div>
                {this.renderExited()}

                <div className="row" style={{ "padding": "7px" }}>
                    <div className="col-9">
                        { !this.state.isReadOnly ? <button type="button" className="btn btn-success" onClick={() => { this.saveContent(); }}>Save</button> : null }
                        { !this.state.isStandalone ? <button type="button" className="btn btn-primary" onClick={() => { this.runSnippet(); }}>Run snippet</button> : null }
                        { !this.state.isReadOnly ? <button type="button" className="btn btn-primary" onClick={() => { this.saveContentAndExit(); }}>Save & exit</button> : null }
                        <button type="button" className="btn btn-danger" onClick={() => { this.exit(); }}>Exit</button>
                    </div>
                    <div className="col-3">
                        <div className="form-check form-check-inline">
                            <input className="form-check-input" type="checkbox" checked={this.state.showText} id="showTextCheckbox"
                                   onChange={event => { this.changeText(event); }} />
                            <label className="form-check-label" htmlFor="showTextCheckbox">Text</label>
                        </div>

                        <div className="form-check form-check-inline">
                            <input className="form-check-input" type="checkbox" checked={this.state.showRendered} id="showRenderedheckbox"
                                   onChange={event => { this.changeRendered(event); }} />
                            <label className="form-check-label" htmlFor="showRenderedheckbox">Rendered</label>
                        </div>
                    </div>
                </div>
                {this.renderSuccess()}
                {this.renderError()}
                {this.renderSnippetOutput()}
                <div className="row">
                    {this.renderText()}
                    {this.renderMarkdown()}
                </div>
            </div>
        );
    }

    renderSuccess() {
        if (this.state.success == null) {
            return;
        }

        return (
            <div className="row">
                <div className="col-4" />
                <div className="alert alert-success col-4 alert-dismissible fade show" role="alert" style={{ margin: "10px" }}>
                    <h4 className="alert-heading">Success</h4>
                    {this.state.success}
                    <button type="button" className="btn-close" data-bs-dismiss="alert" onClick={() => { this.setState({ success: null }); }} />
                </div>
                <div className="col-4" />
            </div>
        );
    }

    renderError() {
        if (this.state.error == null) {
            return;
        }

        return (
            <div className="row">
                <div className="col-4" />
                <div className="alert alert-danger col-4" role="alert" style={{ margin: "10px" }}>
                    <h4 className="alert-heading">Error</h4>
                    {this.state.error}
                </div>
                <div className="col-4" />
            </div>
        );
    }

    renderText() {
        if (!this.state.showText) {
            return null;
        }

        return (
            <div className={this.numViewsVisible() == 2 ? "col-6" : "col"}>
                <AceEditor
                    ref={this.editArea}
                    mode="markdown"
                    theme="textmate"
                    name="editor"
                    editorProps={{ $blockScrolling: true }}
                    value={this.state.content}
                    readOnly={this.state.isReadOnly}
                    onChange={(newValue) => {
                        this.setState({
                            content: newValue
                        });
                    }}
                    width="100%"
                    height="100%"
                    className="editor"
                />
            </div>
        );
    }

    renderMarkdown() {
        if (!this.state.showRendered) {
            return null;
        }

        return (
            <div className={this.numViewsVisible() == 2 ? "col-6" : "col"}>
                <ReactMarkdown
                    className="markdown"
                    children={this.state.content}
                    components={{
                        code({node, inline, className, children, ...props}) {
                            const match = /language-(\w+)/.exec(className || '')
                            return !inline && match ? (
                                <SyntaxHighlighter
                                    {...props}
                                    children={String(children).replace(/\n$/, '')}
                                    language={match[1]}
                                    PreTag="div"
                                />
                            ) : (
                                <code {...props} className={className}>
                                    {children}
                                </code>
                            )
                        }
                    }}
                />
            </div>
        );
    }

    renderSnippetOutput() {
        if (this.state.snippetOutput != null) {
            return (
                <div className="row">
                    <div className="col-4" />
                    <div className="col-4">
                        <div className="card" style={{ marginBottom: "10px", textAlign: "center" }}>
                            <div className="card-body">
                                <h5 className="card-title">
                                    Snippet output

                                    <i className="fas fa-times linkButton" style={{ float: "right" }} onClick={() => { this.closeSnippetOutput(); }} />
                                </h5>

                                <p className="text-monospace snippetOutput">
                                    {this.state.snippetOutput}
                                </p>

                                <button type="button" className="btn btn-success" onClick={() => { this.updateContentUsingSnippet(); }}>Update content</button>
                            </div>
                        </div>
                    </div>
                    <div className="col-4" />
                </div>
            );
        } else {
            return null;
        }
    }

    renderExited() {
        return (
            <div className="modal fade" id="exitedModal" tabIndex={-1} aria-labelledby="exitedModalLabel" aria-hidden="true">
                <div className="modal-dialog">
                    <div className="modal-content">
                        <div className="modal-header">
                            <h1 className="modal-title fs-5" id="exitedModalLabel">WebEditor</h1>
                            <button type="button" className="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                        </div>
                        <div className="modal-body">
                            Web editor has been closed. Please close this browser tab.
                        </div>
                    </div>
                </div>
            </div>
        );
    }

    changeText(event: React.ChangeEvent<HTMLInputElement>) {
        this.setState({
            showText: event.target.checked,
        });
    }

    changeRendered(event: React.ChangeEvent<HTMLInputElement>) {
        this.setState({
            showRendered: event.target.checked,
        });
    }

    closeSnippetOutput() {
        this.setState({
            snippetOutput: null,
            snippetOutputContent: null
        });
    }

    updateContentUsingSnippet() {
        if (this.state.snippetOutputContent != null) {
            this.setState({
                content: this.state.snippetOutputContent
            })
        }
    }

    fetchContent() {
        axios.get(`/api/content?path=${this.props.filePath}`)
            .then(response => {
                this.setState({
                    content: response.data.content,
                    error: null
                });
            }).catch(error => {
                this.setState({
                    error: getErrorMessage(error)
                });
            });
    }

    saveContent(onSuccess?: () => void) {
        this.setState({
            success: null
        });

        axios.put(`/api/content`, { "path": this.props.filePath, "content": this.state.content })
            .then(_ => {
                this.setState({
                    error: null,
                    success: "File saved."
                });

                if (onSuccess) {
                    onSuccess();
                }
            }).catch(error => {
                this.setState({
                    error: getErrorMessage(error)
                });
            });
    }

    runSnippet() {
        this.setState({
            success: null
        });

        axios.post(`/api/run-snippet`, { "content": this.state.content })
            .then(response => {
                this.setState({
                    error: null,
                    snippetOutput: response.data["output"],
                    snippetOutputContent: response.data["newContent"]
                });
            }).catch(error => {
            this.setState({
                error: getErrorMessage(error)
            });
        });
    }

    saveContentAndExit() {
        this.saveContent(() => {
            this.exit();
        });
    }

    exit() {
        axios.post(`/api/stop`)
            .then(_ => {
                this.setState({
                    error: null,
                    isReadOnly: true
                });

                try {
                    window.close();
                } catch (error) {
                    console.log("Failed to close window: " + error);
                }

                try {
                    sendMessageToServer("exit");
                } catch (error) {
                    console.log("Failed to close webview: " + error);
                }

                // @ts-ignore
                let modal = new bootstrap.Modal(document.getElementById("exitedModal"));
                modal.show();
            }).catch(error => {
                this.setState({
                    error: getErrorMessage(error)
                });
        });
    }

    numViewsVisible() {
        return (this.state.showText ? 1 : 0) + (this.state.showRendered ? 1 : 0);
    }
}

function getErrorMessage(error) {
    if (error.response !== undefined) {
        return error.response.data.message;
    } else {
        return "Failed to send request.";
    }
}

function sendMessageToServer(cmd) {
    if (window.external !== undefined) {
        // @ts-ignore
        return window.external.invoke(cmd);
    } else { // @ts-ignore
        if (window.webkit.messageHandlers.external !== undefined) {
            // @ts-ignore
            return window.webkit.messageHandlers.external.postMessage(cmd);
        }
    }
    throw new Error('Failed to locate webkit external handler')
}

ReactDOM.render(
    <WebEditorMain
        filePath={(document.getElementById("file_path") as HTMLInputElement).value}
        isReadOnly={(document.getElementById("is_read_only") as HTMLInputElement).value == "true"}
        isStandalone={(document.getElementById("is_standalone") as HTMLInputElement).value == "true"}
    />,
    document.getElementById("root")
);
