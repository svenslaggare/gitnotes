use std::cell::RefCell;

use comrak::{Arena, ComrakOptions};
use comrak::nodes::{Ast, AstNode, LineColumn, NodeCodeBlock, NodeValue};

pub fn storage<'a>() -> Arena<AstNode<'a>> {
    Arena::new()
}

pub fn parse<'a>(arena: &'a Arena<AstNode<'a>>, content: &str) -> &'a AstNode<'a> {
    comrak::parse_document(
        &arena,
        &content,
        &ComrakOptions::default()
    )
}

pub fn visit_code_blocks<'a, E, F: FnMut(&'a AstNode<'a>) -> Result<(), E>>(root: &'a AstNode<'a>, mut apply: F, show_code: bool, show_output: bool) -> Result<(), E> {
    for current_node in root.children() {
        match current_node.data.borrow().value {
            NodeValue::CodeBlock(ref block) => {
                let is_code = block.info != "output";
                if (is_code && show_code) || (!is_code && show_output) {
                    apply(current_node)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn visit_non_code_blocks<'a, E, F: FnMut(&'a AstNode<'a>) -> Result<(), E>>(root: &'a AstNode<'a>, mut apply: F) -> Result<(), E> {
    for current_node in root.children() {
        match &current_node.data.borrow().value {
            NodeValue::CodeBlock(_) => {}
            NodeValue::Code(_) => {}
            _ => {
                apply(current_node)?;
            }
        }
    }

    Ok(())
}

pub fn ast_to_string<'a>(root: &'a AstNode<'a>) -> std::io::Result<String> {
    let mut output = Vec::new();
    comrak::format_commonmark(root, &ComrakOptions::default(), &mut output)?;
    Ok(String::from_utf8(output).unwrap())
}

pub fn create_output_code_block<'a>(arena: &'a Arena<AstNode<'a>>, output: String) -> &'a mut AstNode::<'a> {
    let mut output_block = NodeCodeBlock::default();
    output_block.info = "output".to_owned();
    output_block.literal = output;
    arena.alloc(AstNode::new(RefCell::new(Ast::new(NodeValue::CodeBlock(output_block), LineColumn::from((0, 0))))))
}