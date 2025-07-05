use bumpalo::Bump;
use rowan::{GreenNode, GreenNodeBuilder, Language, SyntaxNode as RowanSyntaxNode, SyntaxToken as RowanSyntaxToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConTeXtLanguage {}
impl Language for ConTeXtLanguage {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        unsafe { std::mem::transmute(raw.0) }
    }
    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxKind {
    Document,
    Environment,
    Command,
    Text,
    Comment,
    Options,
    Argument,
    Error,
}

pub type SyntaxNode = RowanSyntaxNode<ConTeXtLanguage>;
pub type SyntaxToken = RowanSyntaxToken<ConTeXtLanguage>;

#[derive(Debug)]
pub struct SyntaxTree {
    arena: Bump,        
    green: GreenNode, 
}

impl SyntaxTree {
    pub fn new(arena: Bump, green: GreenNode) -> Self {
        Self { arena, green }
    }
    
    pub fn root(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

pub struct SyntaxTreeBuilder<'a> {
    arena: &'a Bump,   
    builder: GreenNodeBuilder<'a>,
}

impl<'a> SyntaxTreeBuilder<'a> {
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            builder: GreenNodeBuilder::new(),
        }
    }
    
    pub fn start_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(SyntaxKind::to_raw(kind));
    }
    
    pub fn finish_node(&mut self) {
        self.builder.finish_node();
    }
    
    pub fn token(&mut self, kind: SyntaxKind, text: &'a str) {
        let text = self.arena.alloc_str(text);
        self.builder.token(SyntaxKind::to_raw(kind), text);
    }
    
    pub fn finish(self) -> SyntaxTree {
        let arena = Bump::new();
        SyntaxTree::new(
            arena,
            self.builder.finish()
        )
    }
}

impl SyntaxKind {
    fn to_raw(self) -> rowan::SyntaxKind {
        ConTeXtLanguage::kind_to_raw(self)
    }
}
