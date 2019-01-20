use crate::node::Node;
use crate::util::{BSMap, ValueSet};
use std::collections::{HashMap, HashSet, VecDeque};
use std::{cmp, ops};

/// A reference to a node in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeRef(pub(crate) u64);

/// Link metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Link {
    out_prop: usize,
    in_prop: usize,
}

/// A key for the link map, sorted by to_in first and from_out second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LinkKey {
    from_out: NodeRef,
    to_in: NodeRef,
}

impl LinkKey {
    /// Swaps to_in and from_out for the reverse link map.
    fn swap(self) -> LinkKey {
        LinkKey {
            from_out: self.to_in,
            to_in: self.from_out,
        }
    }
}

impl PartialOrd for LinkKey {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LinkKey {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match self.to_in.cmp(&other.to_in) {
            cmp::Ordering::Equal => self.from_out.cmp(&other.from_out),
            x => x,
        }
    }
}

/// Contains links.
#[derive(Debug, Clone)]
struct Links(BSMap<LinkKey, Link, ValueSet>, BSMap<LinkKey, ()>);

impl Links {
    fn new() -> Links {
        Links(BSMap::new(), BSMap::new())
    }

    /// Returns all inputs to a node, i.e. all links that link to an input port on that node.
    pub fn get_ins(&self, key: NodeRef) -> impl Iterator<Item = (NodeRef, Link)> + '_ {
        self.0
            .range_by_key(key, |item| &item.to_in)
            .map(|(key, link)| (key.from_out, *link))
    }

    /// Returns all outputs from a node, i.e. all links that link from an output port on that node.
    pub fn get_outs(&self, key: NodeRef) -> impl Iterator<Item = (NodeRef, Link)> + '_ {
        self.1
            .range_by_key(key, |item| &item.to_in)
            .map(|(key, _)| (key, key.swap()))
            .flat_map(move |(key, swapped)| {
                self.0
                    .range_by(move |item| item.cmp(&swapped))
                    .map(move |item| (key.from_out, item.1))
            })
    }

    /// Inserts a link.
    pub fn insert(&mut self, from_out: NodeRef, to_in: NodeRef, link: Link) {
        self.0.insert_value(LinkKey { from_out, to_in }, link);
        self.1.insert(LinkKey { from_out, to_in }.swap(), ());
    }

    /// Removes a link if it exists. Returns true if a link was removed.
    pub fn remove(&mut self, from_out: NodeRef, to_in: NodeRef, link: Link) -> bool {
        let key = LinkKey { from_out, to_in };
        if let Some(_) = self.0.remove_value(&key, &link) {
            if !self.0.contains_key(&key) {
                // no more links with that key exist; remove from reverse table
                self.1.remove(&key.swap());
            }
            true
        } else {
            false
        }
    }

    /// Removes all links to and from a node.
    pub fn remove_node(&mut self, node: NodeRef) {
        // all links to the node
        let links: Vec<_> = self
            .0
            .range_by_key(node, |probe| &probe.to_in)
            .map(|x| *x)
            .collect();
        for (key, link) in links {
            self.0.remove_value(&key, &link);
            self.1.remove(&key.swap());
        }

        // all links from the node
        let links: Vec<_> = self
            .1
            .range_by_key(node, |probe| &probe.to_in)
            .map(|x| *x)
            .collect();
        for (key, _) in links {
            let links: Vec<_> = self.0.range(key.swap()).map(|x| *x).collect();
            for (key, link) in links {
                self.0.remove_value(&key, &link);
            }
            self.1.remove(&key);
        }
    }

    /// Remaps node properties. Returns true if some links were dropped.
    pub fn remap_with_mapping(
        &mut self,
        nodes: HashSet<NodeRef>,
        mapping: HashMap<usize, usize>,
    ) -> bool {
        let mut links_to_drop = Vec::new();
        for (k, v) in self.0.iter_mut() {
            if nodes.contains(&k.from_out) {
                if let Some(new) = mapping.get(&v.out_prop) {
                    v.out_prop = *new;
                } else {
                    links_to_drop.push((*k, *v));
                    continue;
                }
            }
            if nodes.contains(&k.to_in) {
                if let Some(new) = mapping.get(&v.in_prop) {
                    v.in_prop = *new;
                } else {
                    links_to_drop.push((*k, *v));
                    continue;
                }
            }
        }

        for (k, v) in &links_to_drop {
            self.remove(k.from_out, k.to_in, *v);
        }

        !links_to_drop.is_empty()
    }
}

/// Graph ordering errors.
#[derive(Fail, Debug, Clone, PartialEq)]
pub enum OrderError {
    /// The graph contains a cycle. The participating nodes of the first detected cycle are given in
    /// the argument.
    #[fail(display = "cycle found with nodes {:?}", _0)]
    Cycle(Vec<NodeRef>),
}

struct ToposortState<'a> {
    order: &'a mut Vec<NodeRef>,
    visiting: BSMap<NodeRef, ()>,
    marked: BSMap<NodeRef, ()>,
    io_node: NodeRef,
}

/// A node graph.
#[derive(Debug, Clone)]
pub struct Graph {
    nodes: BSMap<NodeRef, Node>,
    links: Links,
    io_node: NodeRef,
    order: Option<Vec<NodeRef>>,
    dirty_nodes: BSMap<NodeRef, ()>,
}

impl Graph {
    /// Creates a new node graph.
    pub fn new() -> Graph {
        Graph {
            nodes: BSMap::new(),
            links: Links::new(),
            io_node: NodeRef(0),
            order: None,
            dirty_nodes: BSMap::new(),
        }
    }

    /// Invalidates topological sorting.
    fn invalidate_order(&mut self) {
        self.order = None;
    }

    /// Adds a node to the graph and returns a (weak) reference.
    pub fn add_node(&mut self, node: Node) -> NodeRef {
        let next_id = self
            .nodes
            .greatest_key()
            .map_or(Some(1), |k| k.0.checked_add(1));

        let node_ref = if let Some(next_id) = next_id {
            NodeRef(next_id)
        } else {
            // TODO: handle overflow
            unimplemented!("ID overflow");
        };

        self.nodes.insert(node_ref, node);
        self.dirty_nodes.insert(node_ref, ());
        node_ref
    }

    /// Sets the output node.
    pub fn set_output(&mut self, node: NodeRef) {
        self.io_node = node;
        self.invalidate_order();
    }

    /// Returns the output node ID.
    pub fn output(&self) -> NodeRef {
        self.io_node
    }

    /// Returns a reference to a node.
    pub fn node(&self, node: &NodeRef) -> Option<&Node> {
        self.nodes.get(node)
    }

    /// Returns a mutable reference to a node.
    pub fn node_mut(&mut self, node: &NodeRef) -> Option<&mut Node> {
        // there’s no real way of knowing if the node was actually mutated
        // without using a proxy object or something, so just assume it will
        // always be mutated with node_mut
        self.dirty_nodes.insert(*node, ());
        self.nodes.get_mut(node)
    }

    /// Removes a node from the graph, along with any links.
    pub fn remove_node(&mut self, node: NodeRef) -> Option<Node> {
        self.invalidate_order();
        self.links.remove_node(node);
        self.nodes.remove(&node)
    }

    /// Links two node properties.
    pub fn link(&mut self, out_node: NodeRef, out_prop: usize, in_node: NodeRef, in_prop: usize) {
        self.invalidate_order();
        self.links
            .insert(out_node, in_node, Link { out_prop, in_prop });
    }

    /// Returns an iterator over all inputs of a node in (other node, output prop, input prop on
    /// this node) tuples.
    pub fn node_inputs(&self, node: NodeRef) -> impl Iterator<Item = (NodeRef, usize, usize)> + '_ {
        self.links
            .get_ins(node)
            .map(|(node, link)| (node, link.out_prop, link.in_prop))
    }

    /// Returns an iterator over all outputs of a node in (other node, output prop on this node,
    /// input prop) tuples.
    pub fn node_outputs(
        &self,
        node: NodeRef,
    ) -> impl Iterator<Item = (NodeRef, usize, usize)> + '_ {
        self.links
            .get_outs(node)
            .map(|(node, link)| (node, link.out_prop, link.in_prop))
    }

    /// Returns an iterator over all links to an output port on a node in (other node, input prop)
    /// tuples.
    pub fn node_output_links(
        &self,
        node: NodeRef,
        prop: usize,
    ) -> impl Iterator<Item = (NodeRef, usize)> + '_ {
        self.node_outputs(node)
            .filter(move |(_, p, _)| *p == prop)
            .map(|(node, _, prop)| (node, prop))
    }

    /// Removes a link between two node properties. Returns true if a link was removed.
    pub fn unlink(
        &mut self,
        out_node: NodeRef,
        out_prop: usize,
        in_node: NodeRef,
        in_prop: usize,
    ) -> bool {
        self.invalidate_order();
        self.links
            .remove(out_node, in_node, Link { out_prop, in_prop })
    }

    /// Iterates over all nodes.
    pub fn iter_nodes(&self) -> impl Iterator<Item = &(NodeRef, Node)> {
        self.nodes.iter()
    }

    /// Iterates over all links (out, in).
    pub fn iter_links(&self) -> impl Iterator<Item = ((NodeRef, usize), (NodeRef, usize))> + '_ {
        self.links
            .0
            .iter()
            .map(|(k, link)| ((k.from_out, link.out_prop), (k.to_in, link.in_prop)))
    }

    /// False if the evaluation order was invalidated. (Also see [Graph::update_order])
    pub fn has_order(&self) -> bool {
        self.order.is_some()
    }

    /// Recursive topological sort.
    fn toposort(
        &self,
        node: NodeRef,
        state: &mut ToposortState,
    ) -> Result<(), (Vec<NodeRef>, bool)> {
        if state.marked.contains_key(&node) {
            return Ok(());
        }
        if state.visiting.contains_key(&node) {
            if node == state.io_node {
                // is actually the graph input. Don’t follow links
                return Ok(());
            }
            // a cycle was detected
            // if the node references itself, this is a complete cycle
            let is_complete_cycle = self.node_inputs(node).find(|(x, ..)| *x == node).is_some();
            return Err((vec![node], is_complete_cycle));
        }
        state.visiting.insert(node, ());
        for (input, ..) in self.node_inputs(node) {
            self.toposort(input, state).map_err(|mut err| {
                if !err.1 {
                    // add this node if the cycle isn’t complete
                    if err.0.first() == Some(&node) {
                        // the entire cycle has been recorded
                        err.1 = true;
                    } else {
                        err.0.push(node);
                    }
                }
                err
            })?;
        }
        state.marked.insert(node, ());
        state.order.push(node);
        Ok(())
    }

    /// Returns the node evaluation order.
    pub fn order(&self) -> Option<&[NodeRef]> {
        self.order.as_ref().map(|x| &**x)
    }

    /// Updates the evaluation order with respect to the output node.
    pub fn update_order(&mut self) -> Result<(), OrderError> {
        let mut order = Vec::new();

        self.toposort(
            self.io_node,
            &mut ToposortState {
                order: &mut order,
                visiting: BSMap::new(),
                marked: BSMap::new(),
                io_node: self.io_node,
            },
        )
        .map_err(|(cycle, _)| OrderError::Cycle(cycle))?;

        self.order = Some(order);

        Ok(())
    }

    /// Propagates dirtiness of dirty nodes to all connected nodes.
    pub fn propagate_dirtiness(&mut self) {
        let mut dirty_nodes: VecDeque<_> = self.dirty_nodes.iter().map(|(k, _)| *k).collect();
        self.dirty_nodes.clear();

        while !dirty_nodes.is_empty() {
            let node = dirty_nodes.pop_front().unwrap();
            if self.dirty_nodes.contains_key(&node) {
                continue; // process every node once
            } else {
                self.dirty_nodes.insert(node, ());
            }
            for (other_node, _, _) in self.node_outputs(node) {
                dirty_nodes.push_back(other_node);
            }
        }
    }

    /// Returns whether a node is marked dirty.
    pub fn is_dirty(&self, node: &NodeRef) -> bool {
        self.dirty_nodes.contains_key(&node)
    }

    /// Marks a node dirty.
    pub fn mark_dirty(&mut self, node: NodeRef) {
        self.dirty_nodes.insert(node, ());
    }

    /// Marks a node no longer dirty.
    pub fn mark_clean(&mut self, node: &NodeRef) {
        self.dirty_nodes.remove(node);
    }
}

impl ops::Index<NodeRef> for Graph {
    type Output = Node;
    fn index(&self, index: NodeRef) -> &Node {
        self.node(&index).unwrap()
    }
}

impl ops::IndexMut<NodeRef> for Graph {
    fn index_mut(&mut self, index: NodeRef) -> &mut Node {
        self.node_mut(&index).unwrap()
    }
}

#[test]
fn linking() {
    let mut graph = Graph::new();

    let a = graph.add_node(Node::empty("a".into()));
    let b = graph.add_node(Node::empty("b".into()));

    graph.link(a, 0, b, 0);
    graph.link(a, 0, b, 0);
    graph.link(a, 1, b, 0);
    graph.link(a, 1, b, 0);
    graph.link(a, 0, b, 1);
    graph.link(a, 1, b, 1);

    // links must have been deduplicated
    assert_eq!(graph.links.0.len(), 4);

    // only two nodes were linked, so there must be only one entry in the reverse link table
    assert_eq!(graph.links.1.len(), 1);
}

#[test]
fn ordering() {
    let mut graph = Graph::new();

    // a --> b --> c -.             f --> g
    // |           ^   '-.-> e
    // '-----> d --'-----'
    //         |
    //         '--> h

    let a = graph.add_node(Node::empty("a".into()));
    let b = graph.add_node(Node::empty("b".into()));
    let c = graph.add_node(Node::empty("c".into()));
    let d = graph.add_node(Node::empty("d".into()));
    let e = graph.add_node(Node::empty("e".into()));
    let f = graph.add_node(Node::empty("f".into()));
    let g = graph.add_node(Node::empty("g".into()));
    let h = graph.add_node(Node::empty("h".into()));

    graph.link(a, 0, b, 0);
    graph.link(b, 0, c, 0);
    graph.link(c, 0, e, 0);
    graph.link(a, 0, d, 0);
    graph.link(d, 0, c, 0);
    graph.link(d, 0, e, 0);
    graph.link(d, 0, h, 0);
    graph.link(f, 0, g, 0);

    graph.set_output(e);
    graph.update_order().unwrap();

    {
        let order = graph.order().unwrap();

        macro_rules! assert_order {
            ($a:ident < $b:ident) => {
                let a_pos = order.iter().position(|x| *x == $a).unwrap();
                let b_pos = order.iter().position(|x| *x == $b).unwrap();
                assert!(
                    a_pos < b_pos,
                    "Wrong order: {} and {}",
                    stringify!($a),
                    stringify!($b)
                );
            };
        }

        assert_order!(a < b);
        assert_order!(b < c);
        assert_order!(c < e);
        assert_order!(a < d);
        assert_order!(d < c);
        assert_order!(d < e);

        // f, g, and h must not be included
        assert!(!order.contains(&f));
        assert!(!order.contains(&g));
        assert!(!order.contains(&h));
    }

    // create a cycle by linking h to a
    graph.link(h, 0, a, 0);
    match graph.update_order() {
        Ok(_) => panic!("Graph did not detect a cycle"),
        Err(OrderError::Cycle(cycle)) => {
            assert_eq!(cycle.len(), 3);
            assert!(cycle.contains(&a));
            assert!(cycle.contains(&d));
            assert!(cycle.contains(&h));
        }
    }
}
