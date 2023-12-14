"use strict";
var __extends = (this && this.__extends) || (function () {
    var extendStatics = function (d, b) {
        extendStatics = Object.setPrototypeOf ||
            ({ __proto__: [] } instanceof Array && function (d, b) { d.__proto__ = b; }) ||
            function (d, b) { for (var p in b) if (Object.prototype.hasOwnProperty.call(b, p)) d[p] = b[p]; };
        return extendStatics(d, b);
    };
    return function (d, b) {
        if (typeof b !== "function" && b !== null)
            throw new TypeError("Class extends value " + String(b) + " is not a constructor or null");
        extendStatics(d, b);
        function __() { this.constructor = d; }
        d.prototype = b === null ? Object.create(b) : (__.prototype = b.prototype, new __());
    };
})();
var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};
var __rest = (this && this.__rest) || function (s, e) {
    var t = {};
    for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p) && e.indexOf(p) < 0)
        t[p] = s[p];
    if (s != null && typeof Object.getOwnPropertySymbols === "function")
        for (var i = 0, p = Object.getOwnPropertySymbols(s); i < p.length; i++) {
            if (e.indexOf(p[i]) < 0 && Object.prototype.propertyIsEnumerable.call(s, p[i]))
                t[p[i]] = s[p[i]];
        }
    return t;
};
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
var react_1 = __importDefault(require("react"));
var react_dom_1 = __importDefault(require("react-dom"));
var react_ace_1 = __importDefault(require("react-ace"));
require("ace-builds/src-noconflict/mode-markdown");
require("ace-builds/src-noconflict/theme-textmate");
require("brace");
require("brace/ext/searchbox");
var react_markdown_1 = __importDefault(require("react-markdown"));
var react_syntax_highlighter_1 = require("react-syntax-highlighter");
var axios_1 = __importDefault(require("axios"));
var WebEditorMainProps = /** @class */ (function () {
    function WebEditorMainProps() {
    }
    return WebEditorMainProps;
}());
var WebEditorMain = /** @class */ (function (_super) {
    __extends(WebEditorMain, _super);
    function WebEditorMain(props) {
        var _this = _super.call(this, props) || this;
        _this.state = {
            content: "",
            isReadOnly: _this.props.isReadOnly,
            isStandalone: _this.props.isStandalone,
            showCode: !_this.props.isReadOnly,
            showRendered: true,
            success: null,
            error: null,
            snippetOutput: null
        };
        _this.editArea = react_1.default.createRef();
        _this.fetchContent();
        return _this;
    }
    WebEditorMain.prototype.render = function () {
        var _this = this;
        return (react_1.default.createElement("div", null,
            this.renderExited(),
            react_1.default.createElement("div", { className: "row", style: { "padding": "7px" } },
                react_1.default.createElement("div", { className: "col-9" },
                    !this.state.isReadOnly ? react_1.default.createElement("button", { type: "button", className: "btn btn-success", onClick: function () { _this.saveContent(); } }, "Save") : null,
                    !this.state.isStandalone ? react_1.default.createElement("button", { type: "button", className: "btn btn-primary", onClick: function () { _this.runSnippet(); } }, "Run snippet") : null,
                    !this.state.isReadOnly ? react_1.default.createElement("button", { type: "button", className: "btn btn-primary", onClick: function () { _this.saveContentAndExit(); } }, "Save & exit") : null,
                    react_1.default.createElement("button", { type: "button", className: "btn btn-danger", onClick: function () { _this.exit(); } }, "Exit")),
                react_1.default.createElement("div", { className: "col-3" },
                    react_1.default.createElement("div", { className: "form-check form-check-inline" },
                        react_1.default.createElement("input", { className: "form-check-input", type: "checkbox", checked: this.state.showCode, id: "showCodeCheckbox", onChange: function (event) { _this.changeCode(event); } }),
                        react_1.default.createElement("label", { className: "form-check-label", htmlFor: "showCodeCheckbox" }, "Code")),
                    react_1.default.createElement("div", { className: "form-check form-check-inline" },
                        react_1.default.createElement("input", { className: "form-check-input", type: "checkbox", checked: this.state.showRendered, id: "showRenderedheckbox", onChange: function (event) { _this.changeRendered(event); } }),
                        react_1.default.createElement("label", { className: "form-check-label", htmlFor: "showRenderedheckbox" }, "Rendered")))),
            this.renderSuccess(),
            this.renderError(),
            this.renderSnippetOutput(),
            react_1.default.createElement("div", { className: "row" },
                this.renderCode(),
                this.renderMarkdown())));
    };
    WebEditorMain.prototype.renderSuccess = function () {
        var _this = this;
        if (this.state.success == null) {
            return;
        }
        return (react_1.default.createElement("div", { className: "row" },
            react_1.default.createElement("div", { className: "col-4" }),
            react_1.default.createElement("div", { className: "alert alert-success col-4 alert-dismissible fade show", role: "alert", style: { margin: "10px" } },
                react_1.default.createElement("h4", { className: "alert-heading" }, "Success"),
                this.state.success,
                react_1.default.createElement("button", { type: "button", className: "btn-close", "data-bs-dismiss": "alert", onClick: function () { _this.setState({ success: null }); } })),
            react_1.default.createElement("div", { className: "col-4" })));
    };
    WebEditorMain.prototype.renderError = function () {
        if (this.state.error == null) {
            return;
        }
        return (react_1.default.createElement("div", { className: "row" },
            react_1.default.createElement("div", { className: "col-4" }),
            react_1.default.createElement("div", { className: "alert alert-danger col-4", role: "alert", style: { margin: "10px" } },
                react_1.default.createElement("h4", { className: "alert-heading" }, "Error"),
                this.state.error),
            react_1.default.createElement("div", { className: "col-4" })));
    };
    WebEditorMain.prototype.renderCode = function () {
        var _this = this;
        if (!this.state.showCode) {
            return null;
        }
        return (react_1.default.createElement("div", { className: this.numViewsVisible() == 2 ? "col-6" : "col" },
            react_1.default.createElement(react_ace_1.default, { ref: this.editArea, mode: "markdown", theme: "textmate", name: "editor", editorProps: { $blockScrolling: true }, value: this.state.content, readOnly: this.state.isReadOnly, onChange: function (newValue) {
                    _this.setState({
                        content: newValue
                    });
                }, width: "100%", height: "100%", className: "editor" })));
    };
    WebEditorMain.prototype.renderMarkdown = function () {
        if (!this.state.showRendered) {
            return null;
        }
        return (react_1.default.createElement("div", { className: this.numViewsVisible() == 2 ? "col-6" : "col" },
            react_1.default.createElement(react_markdown_1.default, { className: "markdown", children: this.state.content, components: {
                    code: function (_a) {
                        var node = _a.node, inline = _a.inline, className = _a.className, children = _a.children, props = __rest(_a, ["node", "inline", "className", "children"]);
                        var match = /language-(\w+)/.exec(className || '');
                        return !inline && match ? (react_1.default.createElement(react_syntax_highlighter_1.Prism, __assign({}, props, { children: String(children).replace(/\n$/, ''), language: match[1], PreTag: "div" }))) : (react_1.default.createElement("code", __assign({}, props, { className: className }), children));
                    }
                } })));
    };
    WebEditorMain.prototype.renderSnippetOutput = function () {
        var _this = this;
        if (this.state.snippetOutput != null) {
            return (react_1.default.createElement("div", { className: "row" },
                react_1.default.createElement("div", { className: "col-4" }),
                react_1.default.createElement("div", { className: "col-4" },
                    react_1.default.createElement("b", null, "Snippet output"),
                    react_1.default.createElement("i", { className: "fas fa-times linkButton", style: { float: "right" }, onClick: function () { _this.setState({ snippetOutput: null }); } }),
                    react_1.default.createElement("p", { className: "text-monospace snippetOutput" }, this.state.snippetOutput)),
                react_1.default.createElement("div", { className: "col-4" })));
        }
        else {
            return null;
        }
    };
    WebEditorMain.prototype.renderExited = function () {
        return (react_1.default.createElement("div", { className: "modal fade", id: "exitedModal", tabIndex: -1, "aria-labelledby": "exitedModalLabel", "aria-hidden": "true" },
            react_1.default.createElement("div", { className: "modal-dialog" },
                react_1.default.createElement("div", { className: "modal-content" },
                    react_1.default.createElement("div", { className: "modal-header" },
                        react_1.default.createElement("h1", { className: "modal-title fs-5", id: "exitedModalLabel" }, "WebEditor"),
                        react_1.default.createElement("button", { type: "button", className: "btn-close", "data-bs-dismiss": "modal", "aria-label": "Close" })),
                    react_1.default.createElement("div", { className: "modal-body" }, "Web editor has been closed. Please close this browser tab.")))));
    };
    WebEditorMain.prototype.changeCode = function (event) {
        this.setState({
            showCode: event.target.checked,
        });
    };
    WebEditorMain.prototype.changeRendered = function (event) {
        this.setState({
            showRendered: event.target.checked,
        });
    };
    WebEditorMain.prototype.fetchContent = function () {
        var _this = this;
        axios_1.default.get("/api/content?path=".concat(this.props.filePath))
            .then(function (response) {
            _this.setState({
                content: response.data.content,
                error: null
            });
        }).catch(function (error) {
            _this.setState({
                error: getErrorMessage(error)
            });
        });
    };
    WebEditorMain.prototype.saveContent = function (onSuccess) {
        var _this = this;
        this.setState({
            success: null
        });
        axios_1.default.put("/api/content", { "path": this.props.filePath, "content": this.state.content })
            .then(function (_) {
            _this.setState({
                error: null,
                success: "File saved."
            });
            if (onSuccess) {
                onSuccess();
            }
        }).catch(function (error) {
            _this.setState({
                error: getErrorMessage(error)
            });
        });
    };
    WebEditorMain.prototype.runSnippet = function () {
        var _this = this;
        this.setState({
            success: null
        });
        axios_1.default.post("/api/run-snippet", { "content": this.state.content })
            .then(function (response) {
            _this.setState({
                error: null,
                snippetOutput: response.data["output"]
            });
        }).catch(function (error) {
            _this.setState({
                error: getErrorMessage(error)
            });
        });
    };
    WebEditorMain.prototype.saveContentAndExit = function () {
        var _this = this;
        this.saveContent(function () {
            _this.exit();
        });
    };
    WebEditorMain.prototype.exit = function () {
        var _this = this;
        axios_1.default.post("/api/stop")
            .then(function (_) {
            _this.setState({
                error: null,
                isReadOnly: true
            });
            try {
                window.close();
            }
            catch (error) {
                console.log("Failed to close window: " + error);
            }
            try {
                sendMessageToServer("exit");
            }
            catch (error) {
                console.log("Failed to close webview: " + error);
            }
            // @ts-ignore
            var modal = new bootstrap.Modal(document.getElementById("exitedModal"));
            modal.show();
        }).catch(function (error) {
            _this.setState({
                error: getErrorMessage(error)
            });
        });
    };
    WebEditorMain.prototype.numViewsVisible = function () {
        return (this.state.showCode ? 1 : 0) + (this.state.showRendered ? 1 : 0);
    };
    return WebEditorMain;
}(react_1.default.Component));
function getErrorMessage(error) {
    if (error.response !== undefined) {
        return error.response.data.message;
    }
    else {
        return "Failed to send request.";
    }
}
function sendMessageToServer(cmd) {
    if (window.external !== undefined) {
        // @ts-ignore
        return window.external.invoke(cmd);
    }
    else { // @ts-ignore
        if (window.webkit.messageHandlers.external !== undefined) {
            // @ts-ignore
            return window.webkit.messageHandlers.external.postMessage(cmd);
        }
    }
    throw new Error('Failed to locate webkit external handler');
}
react_dom_1.default.render(react_1.default.createElement(WebEditorMain, { filePath: document.getElementById("file_path").value, isReadOnly: document.getElementById("is_read_only").value == "true", isStandalone: document.getElementById("is_standalone").value == "true" }), document.getElementById("root"));
