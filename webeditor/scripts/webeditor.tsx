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
import {OptionalRender} from "./helpers";

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

    selectedFile: File;
    linkText: string;
    linkLink: string;
}

class WebEditorMain extends React.Component<WebEditorMainProps, WebEditorMainState> {
    editArea: React.RefObject<AceEditor>;
    addResourceModal: any;
    addLinkModal: any;

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
            snippetOutputContent: null,

            selectedFile: null,
            linkText: "",
            linkLink: ""
        };

        this.editArea = React.createRef();
        this.addResourceModal = null;
        this.addLinkModal = null;
        this.fetchContent();
    }

    render() {
        return (
            <div>
                {this.renderExited()}
                {this.renderAddResourceModal()}
                {this.renderAddLinkModal()}

                <div className="row" style={{ "padding": "7px" }}>
                    <div className="col-9">
                        {this.renderSaveExit()}
                    </div>
                    <div className="col-3">
                        <div className="form-check form-check-inline">
                            <input
                                className="form-check-input" type="checkbox" checked={this.state.showText} id="showTextCheckbox"
                                onChange={event => { this.changeText(event); }}
                            />
                            <label className="form-check-label" htmlFor="showTextCheckbox">Text</label>
                        </div>

                        <div className="form-check form-check-inline">
                            <input
                                className="form-check-input" type="checkbox" checked={this.state.showRendered} id="showRenderedheckbox"
                                onChange={event => { this.changeRendered(event); }}
                            />
                            <label className="form-check-label" htmlFor="showRenderedheckbox">Rendered</label>
                        </div>
                    </div>
                </div>
                {this.renderActions()}
                {this.renderSuccess()}
                {this.renderError()}
                {this.renderSnippetOutput()}
                {this.renderEditorCommands()}
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

    renderSaveExit() {
        return (
            <span>
                <OptionalRender
                    condition={!this.state.isReadOnly}
                    trueContent={<button type="button" className="btn btn-success" onClick={() => { this.saveContent(); }}>Save</button>}
                    falseContent={null}
                />

                <OptionalRender
                    condition={!this.state.isReadOnly}
                    trueContent={<button type="button" className="btn btn-primary" onClick={() => { this.saveContentAndExit(); }}>Save & exit</button>}
                    falseContent={null}
                />

                <button type="button" className="btn btn-danger" onClick={() => { this.exit(); }}>Exit</button>
            </span>
        );
    }

    renderActions() {
        if (this.state.isStandalone) {
            return null;
        }

        return (
            <div className="row">
                <div className="col-4" />
                <div className="col-4 centerContent">
                    <span>
                        <button type="button" className="btn btn-primary" onClick={() => { this.runSnippet(); }}>Run snippet</button>
                        <OptionalRender
                            condition={!this.state.isReadOnly}
                            trueContent={
                                <button type="button" className="btn btn-primary" onClick={() => { this.showAddResourceModel(); }}>Add resource</button>
                            }
                            falseContent={null}
                        />
                        <button type="button" className="btn btn-primary" onClick={() => { this.convertToPDF(); }}>Convert to PDF</button>
                    </span>
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

                                { !this.state.isReadOnly ? <button type="button" className="btn btn-success" onClick={() => { this.updateTextUsingSnippet(); }}>Update text</button> : null }
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

    renderEditorCommands() {
        return (
            <span>
                <i title="Add bold text" className="editorButton fa-solid fa-bold" onClick={() => { this.addBold(); }}></i>
                <i title="Add italic text" className="editorButton fa-solid fa-italic" onClick={() => { this.addItalic(); }}></i>
                <i title="Add link" className="editorButton fa-solid fa-link" onClick={() => { this.showAddLinkModel(); }}></i>
                <span className="separator">|</span>
                <i title="Add unordered list" className="editorButton fa-solid fa-list-ul" onClick={() => { this.addUnorderedList(); }}></i>
                <i title="Add ordered list" className="editorButton fa-solid fa-list-ol" onClick={() => { this.addOrderedList(); }}></i>
                <span className="separator">|</span>
                <i title="Add Python code block" className="editorButton fa-brands fa-python" onClick={() => { this.addCode("python"); }} />
                <i title="Add Bash code block" className="editorButton fa-solid fa-terminal" onClick={() => { this.addCode("bash"); }} />
                <i title="Add JavaScript code block" className="editorButton fa-brands fa-js" onClick={() => { this.addCode("javascript"); }} />
                <img
                    title="Add TypeScript code block" className="editorButton svgIcon" onClick={() => { this.addCode("typescript"); }}
                    src="/content/images/typescript.svg"
                />
                <img
                    title="Add C++ code block" className="editorButton svgIcon" onClick={() => { this.addCode("cpp"); }}
                    src="/content/images/cpp.svg"
                />
                <i title="Add Rust code block" className="editorButton fa-brands fa-rust" onClick={() => { this.addCode("rust"); }} />
                <i title="Add code block" className="editorButton fa-solid fa-code" onClick={() => { this.addCode(); }} />      
            </span>
        );
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

    updateTextUsingSnippet() {
        if (this.state.snippetOutputContent != null) {
            this.setState({
                content: this.state.snippetOutputContent
            })
        }
    }

    addBold() {
        this.insertAround("**", "**");
    }

    addItalic() {
        this.insertAround("*", "*");
    }

    addLink() {
        this.insertAtEnd(`\n[${this.state.linkText}](${this.state.linkLink})`);

        this.setState({
            linkText: "",
            linkLink: ""
        });

        this.hideAddLinkModal();
    }

    addUnorderedList() {
        this.insertAtEnd("\n* Item\n");
    }

    addOrderedList() {
        this.insertAtEnd("\n1. Item\n");
    }

    addCode(language = "text") {
        this.insertAtEnd("\n```" + language + "\nCode\n```");
    }

    insertAround(begin: string, end: string) {
        let editor = this.editArea.current.editor;
        editor.session.insert(editor.selection.getRange().end, begin);
        editor.session.insert(editor.selection.getRange().start, end);
    }

    insertAtEnd(text: string) {
        let editor = this.editArea.current.editor;
        editor.session.insert({ row: editor.session.getLength(), column: 0 }, text);
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

    showAddResourceModel() {
        // @ts-ignore
        this.addResourceModal = new bootstrap.Modal(document.getElementById("addResourceModal"));
        this.addResourceModal.show();
    }

    hideAddResourceModal() {
        if (this.addResourceModal != null) {
            this.addResourceModal.hide();
        }
    }

    renderAddResourceModal() {
        return (
            <div className="modal" id="addResourceModal" tabIndex={-1} aria-labelledby="addResourceModalLabel" aria-hidden="true">
                <div className="modal-dialog">
                    <div className="modal-content">
                        <div className="modal-header">
                            <h1 className="modal-title fs-5" id="addResourceModalLabel">Add resource</h1>
                            <button type="button" className="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                        </div>
                        <div className="modal-body">
                            <input type="file" onChange={(event) => { this.onFileChanged(event); }} />

                            <br />
                            <br />
                            <button type="button" className="btn btn-primary" onClick={() => { this.addResource(); }}>Upload</button>
                        </div>
                    </div>
                </div>
            </div>
        );
    }

    convertToPDF() {
        let url = `/convert-to-pdf?path=${this.props.filePath}`
        window.open(url, "_blank");
    }

    showAddLinkModel() {
        // @ts-ignore
        this.addLinkModal = new bootstrap.Modal(document.getElementById("addLinkModal"));
        this.addLinkModal.show();
    }

    hideAddLinkModal() {
        if (this.addLinkModal != null) {
            this.addLinkModal.hide();
        }
    }

    renderAddLinkModal() {
        return (
            <div className="modal" id="addLinkModal" tabIndex={-1} aria-labelledby="addLinkModalLabel" aria-hidden="true">
                <div className="modal-dialog">
                    <div className="modal-content">
                        <div className="modal-header">
                            <h1 className="modal-title fs-5" id="addLinkModalLabel">Add link</h1>
                            <button type="button" className="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                        </div>
                        <div className="modal-body">
                            <div className="form-group">
                                <label htmlFor="addLinkText">Text</label>
                                <input
                                    type="text" className="form-control" id="addLinkText" placeholder="Text"
                                    defaultValue={this.state.linkText}
                                    onChange={event => { this.setState({ linkText: event.target.value }) }}
                                />
                            </div>

                            <br/>

                            <div className="form-group">
                                <label htmlFor="addLinkLink">Link</label>
                                <input
                                    type="text" className="form-control" id="addLinkLink" placeholder="URL"
                                    defaultValue={this.state.linkLink}
                                    onChange={event => { this.setState({ linkLink: event.target.value }) }}
                                />
                            </div>

                            <br/>
                            <button
                                type="button" className="btn btn-primary"
                                disabled={!(this.state.linkText.length > 0 && this.state.linkLink.length > 0)}
                                onClick={() => { this.addLink(); }}>
                                Add
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        );
    }

    onFileChanged(event: React.ChangeEvent<HTMLInputElement>) {
        this.setState({
            selectedFile: event.target.files[0]
        });
    }

    addResource() {
        if (this.state.selectedFile != null) {
            let formData = new FormData();
            formData.append("file", this.state.selectedFile, this.state.selectedFile.name);

            axios.post("/api/add-resource", formData)
                .then(response => {
                    this.hideAddResourceModal();

                    let editor = this.editArea.current.editor;
                    editor.session.insert(
                        {row: editor.session.getLength(), column: 0},
                        `\n![](resource/${this.state.selectedFile.name})`
                    );

                    this.setState({
                        error: null,
                        selectedFile: null
                    });
                }).catch(error => {
                    this.setState({
                        error: getErrorMessage(error)
                    });
                });
        }
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

ReactDOM.render(
    <WebEditorMain
        filePath={(document.getElementById("file_path") as HTMLInputElement).value}
        isReadOnly={(document.getElementById("is_read_only") as HTMLInputElement).value == "true"}
        isStandalone={(document.getElementById("is_standalone") as HTMLInputElement).value == "true"}
    />,
    document.getElementById("root")
);
