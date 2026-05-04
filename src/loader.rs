use std::{collections::{HashMap, HashSet}, fs};

use crate::{AzimuthFlags, analyzer::CompileError, lexer::{self, Span}, parser::{self, Annotation, Expression, Identifier, ParseError, ParsedAtlas, RawAttachment, RawMapping, ShapeExpression, Statement}};

#[derive(Debug, Clone)]
pub enum LoadError {
    Error { span: Span, message: String },
    FileNotFound { span: Span, location: String },
    ParseError { location: String, error: ParseError },
    LexerError { location: String, error: ParseError },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Error { span, message } =>
                write!(f, "{}: {}", span, message),
                
            LoadError::FileNotFound { span, location } =>
                write!(f, "{}: File not found: {}", span, location),
                
            LoadError::ParseError { location, error } =>
                write!(f, "Could not parse file: {}\n{}", location, error),
                
            LoadError::LexerError { location, error } =>
                write!(f, "Could not tokenize file: {}\n{}", location, error),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedAzimuth {
    pub name: Identifier,
    pub id: u32,
    pub flags: AzimuthFlags,
    pub kind: ShapeExpression,
    pub default_value: Option<Expression>,
}

pub type Filename = String;

#[derive(Debug, Clone, Default)]
pub enum NamespaceKind {
    #[default] Namespace,
    Shape{parents: Vec<RawAttachment>, generics: Vec<(ShapeExpression, Vec<ShapeExpression>)>},
    Atlas
}

pub type NamespaceId = Identifier;

#[derive(Debug, Clone, Default)]
pub struct Namespace {
    pub span: Span,
    pub name: NamespaceId,
    pub id: u32,
    pub kind: NamespaceKind,
    pub children: Vec<NamespaceId>,
    pub azimuths: Vec<LoadedAzimuth>,
    pub dependencies: Vec<NamespaceId>,
    pub aliases: Vec<(Identifier, ShapeExpression)>,
    pub annotations: Vec<Annotation>,
}

impl Namespace {
    pub fn get_azimuth(&self, identifier: &Identifier) -> Option<&LoadedAzimuth> {
        self.azimuths.iter().find(|az| &az.name == identifier)
    }

    pub fn has_static(&self) -> bool {
        for azimuth in &self.azimuths {
            if azimuth.flags.is_static { return true }
        }
        false
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AtlasLocation {
    LocalFile{ path:String },
    LocalDirectory{ path:String },
    LocalSubspace{ path:String, subspace:NamespaceId },
    DiskDepdencency{ path:String },
    NetDependency{ url:String, version:String },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ResolvedAtlasLocation {
    File(String),
    Dir(Vec<String>),
    SubspaceFile(String, Identifier),
}

#[derive(Debug, Clone)]
pub struct AtlasMappingFlags {
    pub hidden: bool,
    pub trailhead: bool,
    pub lazy: bool,
}

#[derive(Debug, Clone)]
pub struct AtlasMapping {
    pub from: Identifier,
    pub to: AtlasLocation,
    pub flags: AtlasMappingFlags,
}

pub struct Loader {
    pub source_dir: String,
    pub files: HashMap<AtlasLocation, Vec<Statement>>,
    pub load_order: Vec<(AtlasLocation, Identifier, u32)>,
    pub namespaces: Vec<Namespace>,
    pub next_az_id: u32,
    pub next_ns_id: u32,
    pub extensions: Vec<Namespace>,
}

impl Loader {

    pub fn new(source_dir: &str) -> Self {
        let root =  Namespace {
            span: Span::new(0,0, source_dir.to_string()),
            name: "".to_string(),
            id: 0,
            kind: NamespaceKind::Atlas,
            children: Vec::new(),
            azimuths: Vec::new(),
            dependencies: Vec::new(),
            aliases: Vec::new(),
            annotations: Vec::new(),
        };
        Loader { 
            source_dir: source_dir.to_string(),
            files: HashMap::new(), 
            namespaces: [root].to_vec(),
            load_order: Vec::new(),
            next_az_id: 0,
            next_ns_id: 0,
            extensions: Vec::new(),
        }
    }

    pub fn next_azimuth_id(&mut self) -> u32 {
        let id = self.next_az_id;
        self.next_az_id += 1;
        id
    }

    pub fn next_namespace_id(&mut self) -> u32 {
        let id = self.next_ns_id;
        self.next_ns_id += 1;
        id
    }

    pub fn load_program(&mut self, atlas_path: &str) -> Result<(), LoadError> {
        let atlas = self.load_atlas(atlas_path)?;
        let mut root_children = Vec::new();

        let root = self.namespaces.get_mut(0).unwrap();
        root.name = atlas.name.clone();

        for mapping in atlas.mappings {
            let ast = match self.load_atlas_location(Span::new(0,0,atlas_path.to_string()), mapping.to.clone())? {
                Some(ast) => ast,
                None => continue,
            };

            let new_span = Span::new(0,0, match &mapping.to {
                AtlasLocation::LocalFile { path } => path.clone(),
                _ => todo!()
            });
            
            self.files.insert(mapping.to.clone(), ast);
            let statements = self.files.get(&mapping.to).unwrap().clone();

            let name = format!("{}::{}", atlas.name, mapping.from);
            //println!("Creating namespace: {}", name);
            let namespace = self.load_namespace(new_span, name.clone(), statements)?;
            let id = namespace.id;
            root_children.push(name.clone());
            self.load_order.push((mapping.to, name, id));
        }

        let root = self.namespaces.get_mut(0).unwrap();
        root.children.append(&mut root_children);

        for extension in &mut self.extensions.clone() {
            self.apply_extension(extension)?;
        }

        Ok(())
    }

    pub fn load_namespace(&mut self, span:Span, identifier:Identifier, statements:Vec<Statement>) -> Result<Namespace, LoadError> {
        let mut children = Vec::new();
        let mut azimuths = Vec::new();
        let mut dependencies = Vec::new();
        let mut aliases = Vec::new();

        for statement in statements {
            match statement {
                Statement::Using { package, .. } => {
                    let name = format!("{}::{}", self.namespaces.get(0).unwrap().name, package);
                    dependencies.push(name);
                }
                Statement::Alias { new, target, .. } => {
                    aliases.push((new, target));
                }
                Statement::DeclareShape { span, name, slot_ids, parents, generics, extension, annotations, .. } => {
                    let name = format!("{}::{}", identifier, name);
                    let azimuths: Vec<LoadedAzimuth> = slot_ids.iter()
                        .map(|raw| LoadedAzimuth{
                            name:raw.name.clone(), 
                            id: self.next_azimuth_id(),
                            kind:raw.value_type.clone(),
                            default_value: raw.set_value.clone(),
                            flags: raw.flags.clone(),
                        }).collect();
                    let namespace = Namespace { 
                        span,
                        name:name.clone(),
                        id: self.next_namespace_id(), 
                        kind:NamespaceKind::Shape{ parents, generics}, 
                        children:Vec::new(), 
                        dependencies:dependencies.clone(),
                        aliases:Vec::new(),
                        azimuths,
                        annotations
                    };
                    
                    if extension {
                        self.extensions.push(namespace);
                    } else {
                        children.push(name);
                        self.namespaces.push(namespace);
                    }
                }
                Statement::Namespace { span, name, content, .. } => {
                    let name = format!("{}::{}", identifier, name);
                    self.load_namespace(span, name.clone(), content)?;
                    children.push(name);
                }
                Statement::DeclareAzimuth { azimuth, .. } => {
                    azimuths.push(LoadedAzimuth{
                        name:azimuth.name.clone(), 
                        id: self.next_azimuth_id(),
                        kind: azimuth.value_type.clone(),
                        default_value: azimuth.set_value.clone(),
                        flags: azimuth.flags.clone(),
                    });
                }
                Statement::Block ( statements ) => {
                    let namespace = self.load_namespace(span.clone(), identifier.clone(), statements)?;
                    for child in namespace.children {
                        children.push(child);
                    }
                    for azimuth in namespace.azimuths {
                        azimuths.push(azimuth);
                    }
                    for dependency in namespace.dependencies {
                        dependencies.push(dependency);
                    }
                }
                _ => {}
            }
        }
        let namespace = Namespace{span, name:identifier, id: self.next_namespace_id(), kind:NamespaceKind::Namespace, children, azimuths, dependencies, aliases, annotations:Vec::new()};
        self.namespaces.push(namespace.clone());
        Ok(namespace)
    }

    pub fn load_atlas(&self, atlas_path: &str) -> Result<ParsedAtlas, LoadError> {
        let atlas_source = match fs::read_to_string(format!("{}/{}", self.source_dir, atlas_path)) {
            Err(_) => return Err(LoadError::FileNotFound{span:Span::new(0,0, atlas_path.to_string()), location:atlas_path.to_string()}),
            Ok(source) => source,
        };
        let atlas_tokens = match lexer::tokenize(&atlas_source, atlas_path.to_string(), true) {
            Err(error) => return Err(LoadError::LexerError{location:atlas_path.to_string(), error}),
            Ok(tokens) => tokens,
        };
        let atlas = match parser::parse_atlas_file(atlas_tokens) {
            Err(error) => return Err(LoadError::ParseError{location:atlas_path.to_string(), error}),
            Ok(ast) => ast,
        };
        Ok(atlas)
    }

    pub fn load_atlas_location(&self, span: Span, location: AtlasLocation) -> Result<Option<Vec<Statement>>, LoadError> {
        match location {
            AtlasLocation::DiskDepdencency{ path } => self.load_file(span, path),
            AtlasLocation::LocalFile{ path } => self.load_file(span, format!("{}/{}", self.source_dir, path)),
            _ => todo!()
        }
    }

    pub fn resolve_atlas_location(&self, span: Span, location: AtlasLocation) -> Result<ResolvedAtlasLocation, LoadError> {
        let resolved = match location {
            AtlasLocation::LocalFile{ path } => ResolvedAtlasLocation::File(format!("{}/{}", self.source_dir, path)),
            AtlasLocation::LocalSubspace{ path, subspace } => ResolvedAtlasLocation::SubspaceFile(format!("{}/{}", self.source_dir, path), subspace),
            AtlasLocation::LocalDirectory{ path } => {
                todo!()
            },
            AtlasLocation::DiskDepdencency{ path } => {
                todo!()
            },
            AtlasLocation::NetDependency{ url, version } => {
                todo!()
            },
        };
        Ok(resolved)
    }

    pub fn load_file(&self, span: Span, location: String) -> Result<Option<Vec<Statement>>, LoadError> {
        //if location.subspace.is_some() { todo!() }
        //if self.files.contains_key(&location) { return Ok(None) }

        let source = fs::read_to_string(location.clone())
            .map_err(|_| LoadError::FileNotFound{span, location:location.clone()})?;

        let tokens = lexer::tokenize(&source, location.clone(), false)
            .map_err(|error| LoadError::LexerError{location:location.clone(), error})?;

        let ast = parser::parse(tokens)
            .map_err(|error| LoadError::ParseError{location:location.clone(), error})?;

        Ok(Some(ast))
    }

    pub fn get_azimuths(&self, name:Identifier, using:Vec<NamespaceId>) -> Vec<(NamespaceId, &LoadedAzimuth)> {
        let mut azimuths = Vec::new();

        for namespace in self.get_namespaces(using){
            
            if let Some(found) = namespace.get_azimuth(&name) {
                let found_tree = format!("{}::{}", namespace.name.clone(), name);
                azimuths.push((found_tree, found));
            }
        }
        azimuths
    }

    pub fn get_namespaces_matching(&self, identifier: Identifier, using:Vec<NamespaceId>) -> Vec<&Namespace> {
        let mut namespaces = Vec::new();
        let mut seen = HashSet::new();

        for namespaceid in using {
            let name = format!("{}::{}", namespaceid, identifier);
            for namespace in &self.namespaces {
                if !seen.contains(&name) && namespace.name == name {
                    namespaces.push(namespace);
                    seen.insert(name.clone());
                } else if !seen.contains(&identifier) && namespace.name == identifier {
                    namespaces.push(namespace);
                    seen.insert(identifier.clone());
                }
            }
        }
        namespaces
    }

    pub fn get_namespaces(&self, using:Vec<NamespaceId>) -> Vec<&Namespace> {
        let mut namespaces = Vec::new();
        let mut seen = HashSet::new();

        for namespaceid in using {
            for namespace in &self.namespaces {
                if !seen.contains(&namespaceid) && namespace.name == namespaceid {
                    namespaces.push(namespace);
                    seen.insert(namespaceid.clone());
                }
            }
        }
        namespaces
    }

    pub fn get_single_namespace(&self, span:Span, child:&NamespaceId) -> Result<&Namespace, CompileError> {
        let mut found = Vec::new();
        for namespace in &self.namespaces {
            if namespace.name == *child {
                found.push(namespace);
            }
        }
        if found.len() == 0 {
            return Err(CompileError::Error{span, message:format!("Shape not found: {}", child)});
        }
        else if found.len() > 1 {
            return Err(CompileError::Error{span, message:format!("Ambiguous extension: {:?}", found)});
        }
        Ok(found[0])
    }

    pub fn get_namespace_mut(&mut self, path:NamespaceId) -> Option<&mut Namespace> {
        for namespace in &mut self.namespaces {
            if namespace.name == path {
                return Some(namespace)
            }
        }
        None
    }

    pub fn apply_extension(&mut self, extension:&mut Namespace) -> Result<(), LoadError> {
        let path = {
            let found = self.get_namespaces_matching(extension.name.clone(), extension.dependencies.clone());
            if found.len() == 0 {
                return Err(LoadError::Error{span:extension.span.clone(), message:format!("Shape not found: {} with path {:?}", extension.name, extension.dependencies)});
            }
            else if found.len() > 1 {
                return Err(LoadError::Error{span:extension.span.clone(), message:format!("Ambiguous extension: {:?}", found)});
            }
            found[0].name.clone()
        };

        let (ext_parents, ext_generics) = match &mut extension.kind {
            NamespaceKind::Shape { parents, generics } => (parents, generics),
            _ => unreachable!(),
        };
        
        let base = match self.get_namespace_mut(path.clone()) {
            None => return Err(LoadError::Error{span:extension.span.clone(), message:format!("Shape somehow not found: {} with path {:?}", extension.name, path)}),
            Some(base) => base,
        };
        
        match &mut base.kind {
            NamespaceKind::Shape{ parents, generics } => {
                parents.append(ext_parents);
                generics.append(ext_generics);
                base.azimuths.append(&mut extension.azimuths);
                base.children.append(&mut extension.children);
            }
            _ => return Err(LoadError::Error{span:extension.span.clone(), message:format!("Extension for non-shape: {:?}", base)}),
        }
        
        Ok(())
    }

}

pub fn load(source_dir: &str, atlas_path: &str) -> Result<Loader, LoadError> {
    let mut loader = Loader::new(source_dir);
    loader.load_program(atlas_path)?;
    //println!("\nLoaded namespaces: {:?}\n", loader.namespaces);
    Ok(loader)
}