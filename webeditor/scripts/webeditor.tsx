import React from "react";
import ReactDOM from 'react-dom'

import AceEditor from "react-ace";
import "ace-builds/src-noconflict/mode-markdown";
import "ace-builds/src-noconflict/theme-textmate";

import ReactMarkdown from "react-markdown";

import axios from "axios";

class WebEditorMainProps {
    filePath: string;
}

interface WebEditorMainState {
    content: string;
    showCode: boolean;
    showMarkdown: boolean;

    success: string;
    error: string;
}

class WebEditorMain extends React.Component<WebEditorMainProps, WebEditorMainState> {
    editArea: React.RefObject<any>;

    constructor(props) {
        super(props);

        this.state = {
            content: "",
            showCode: true,
            showMarkdown: true,
            success: null,
            error: null
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
                        <button type="button" className="btn btn-success" onClick={() => { this.saveContent(); }}>Save</button>
                        <button type="button" className="btn btn-primary" onClick={() => { this.saveContentAndExit(); }}>Save & exit</button>
                        <button type="button" className="btn btn-danger" onClick={() => { this.exit(); }}>Exit</button>
                    </div>
                    <div className="col-3">
                        <button type="button" className="btn btn-primary" onClick={() => { this.showOnlyCode(); }}>Code only</button>
                        <button type="button" className="btn btn-primary" onClick={() => { this.showOnlyMarkdown(); }}>Markdown only</button>
                    </div>
                </div>
                {this.renderSuccess()}
                {this.renderError()}
                <div className="row">
                    {this.renderCode()}
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

    renderCode() {
        if (!this.state.showCode) {
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
        if (!this.state.showMarkdown) {
            return null;
        }

        return (
            <div className={this.numViewsVisible() == 2 ? "col-6" : "col"}>
                <ReactMarkdown className="markdown">{this.state.content}</ReactMarkdown>
            </div>
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

    showOnlyCode() {
        this.setState({
            showCode: true,
            showMarkdown: !this.state.showMarkdown
        });
    }

    showOnlyMarkdown() {
        this.setState({
            showCode: !this.state.showCode,
            showMarkdown: true
        });
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

    saveContentAndExit() {
        this.saveContent(() => {
            this.exit();
        });
    }

    exit() {
        axios.post(`/api/stop`)
            .then(_ => {
                this.setState({
                    error: null
                });

                // @ts-ignore
                let modal = new bootstrap.Modal(document.getElementById("exitedModal"));
                modal.show()
            }).catch(error => {
                this.setState({
                    error: getErrorMessage(error)
                });
        });
    }

    numViewsVisible() {
        return (this.state.showCode ? 1 : 0) + (this.state.showMarkdown ? 1 : 0);
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
    <WebEditorMain filePath={(document.getElementById("file_path") as HTMLInputElement).value} />,
    document.getElementById("root")
);
