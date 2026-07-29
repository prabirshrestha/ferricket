use super::*;

pub(super) fn flat_rows(
    store: &TicketStore,
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let tickets = &store.tickets;
    let mut visible = tickets
        .iter()
        .enumerate()
        .filter(|(_, ticket)| {
            ticket_matches_view(ticket, store, view)
                && query::matches(ticket, tickets, filter, current_user)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    visible.sort_by(|left, right| {
        let left = &tickets[*left];
        let right = &tickets[*right];
        (left.priority(), left.id()).cmp(&(right.priority(), right.id()))
    });
    visible
        .into_iter()
        .map(|ticket| Row {
            ticket,
            depth: 0,
            last: true,
        })
        .collect()
}

pub(super) fn grouped_rows(
    store: &TicketStore,
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let rows = flat_rows(store, filter, view, current_user);
    ["in_progress", "open", "closed"]
        .into_iter()
        .flat_map(|status| {
            rows.iter()
                .filter(move |row| store.tickets[row.ticket].status() == status)
                .cloned()
        })
        .collect()
}

pub(super) fn dependency_rows(
    store: &TicketStore,
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let tickets = &store.tickets;
    let matched = flat_rows(store, filter, view, current_user);
    let mut included = matched
        .iter()
        .map(|row| tickets[row.ticket].id().to_owned())
        .collect::<HashSet<_>>();
    let mut pending = VecDeque::from_iter(included.iter().cloned());
    while let Some(id) = pending.pop_front() {
        let Some(ticket) = store.ticket(&id) else {
            continue;
        };
        for dependency in ticket.array("deps") {
            if store.ticket_by_id.contains_key(&dependency) && included.insert(dependency.clone()) {
                pending.push_back(dependency);
            }
        }
    }
    let mut rows = tickets
        .iter()
        .enumerate()
        .filter(|(_, ticket)| included.contains(ticket.id()))
        .map(|(ticket, _)| Row {
            ticket,
            depth: 0,
            last: true,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let left = &tickets[left.ticket];
        let right = &tickets[right.ticket];
        (left.priority(), left.id()).cmp(&(right.priority(), right.id()))
    });
    rows
}

pub(super) fn layout_columns(
    layout: TicketLayout,
    store: &TicketStore,
    rows: &[Row],
) -> Vec<LayoutColumn> {
    match layout {
        TicketLayout::Board => [
            ("Todo", "Open tickets", "open"),
            ("In progress", "Active work", "in_progress"),
            ("Done", "Completed tickets", "closed"),
        ]
        .into_iter()
        .map(|(label, detail, status)| LayoutColumn {
            label: label.to_owned(),
            detail: detail.to_owned(),
            indices: rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    (store.tickets[row.ticket].status() == status).then_some(index)
                })
                .collect(),
        })
        .collect(),
        TicketLayout::Roadmap | TicketLayout::Dag => dependency_columns(layout, store, rows),
        _ => vec![LayoutColumn {
            label: layout.label().to_owned(),
            detail: format!("{} tickets", rows.len()),
            indices: (0..rows.len()).collect(),
        }],
    }
}

fn dependency_columns(
    layout: TicketLayout,
    store: &TicketStore,
    rows: &[Row],
) -> Vec<LayoutColumn> {
    let by_id = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (store.tickets[row.ticket].id().to_owned(), index))
        .collect::<HashMap<_, _>>();
    let mut indegree = rows
        .iter()
        .map(|row| (store.tickets[row.ticket].id().to_owned(), 0_usize))
        .collect::<HashMap<_, _>>();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    let mut missing = HashSet::new();
    for row in rows {
        let ticket = &store.tickets[row.ticket];
        let mut seen = HashSet::new();
        for dependency in ticket.array("deps") {
            if !seen.insert(dependency.clone()) {
                continue;
            }
            if by_id.contains_key(&dependency) {
                *indegree.entry(ticket.id().to_owned()).or_default() += 1;
                dependents
                    .entry(dependency)
                    .or_default()
                    .push(ticket.id().to_owned());
            } else {
                missing.insert(ticket.id().to_owned());
            }
        }
    }
    let mut queue = indegree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<Vec<_>>();
    queue.sort();
    let mut queue = VecDeque::from(queue);
    let mut stage = queue
        .iter()
        .map(|id| (id.clone(), 0_usize))
        .collect::<HashMap<_, _>>();
    while let Some(id) = queue.pop_front() {
        let next_stage = stage.get(&id).copied().unwrap_or_default() + 1;
        let mut next = dependents.get(&id).cloned().unwrap_or_default();
        next.sort();
        for dependent in next {
            stage
                .entry(dependent.clone())
                .and_modify(|value| *value = (*value).max(next_stage))
                .or_insert(next_stage);
            let remaining = indegree.entry(dependent.clone()).or_default();
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                queue.push_back(dependent);
            }
        }
    }
    let mut unresolved = missing;
    unresolved.extend(
        rows.iter()
            .filter(|row| !stage.contains_key(store.tickets[row.ticket].id()))
            .map(|row| store.tickets[row.ticket].id().to_owned()),
    );
    let mut pending = VecDeque::from_iter(unresolved.iter().cloned());
    while let Some(id) = pending.pop_front() {
        for dependent in dependents.get(&id).into_iter().flatten() {
            if unresolved.insert(dependent.clone()) {
                pending.push_back(dependent.clone());
            }
        }
    }
    let max_stage = stage.values().copied().max().unwrap_or_default();
    let mut columns = (0..=max_stage)
        .filter_map(|column| {
            let indices = rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    let id = store.tickets[row.ticket].id();
                    (stage.get(id) == Some(&column) && !unresolved.contains(id)).then_some(index)
                })
                .collect::<Vec<_>>();
            (!indices.is_empty()).then(|| LayoutColumn {
                label: if layout == TicketLayout::Roadmap {
                    format!("Stage {}", column + 1)
                } else {
                    format!("Layer {}", column + 1)
                },
                detail: if column == 0 {
                    "Ready prerequisites".to_owned()
                } else {
                    format!("After layer {column}")
                },
                indices,
            })
        })
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        columns.push(LayoutColumn {
            label: "Needs attention".to_owned(),
            detail: "Missing or cyclic dependencies".to_owned(),
            indices: rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    unresolved
                        .contains(store.tickets[row.ticket].id())
                        .then_some(index)
                })
                .collect(),
        });
    }
    columns
}

pub(super) fn hierarchy_rows(
    store: &TicketStore,
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let tickets = &store.tickets;
    let by_id = tickets
        .iter()
        .map(|ticket| (ticket.id(), ticket))
        .collect::<HashMap<_, _>>();
    let mut visible = tickets
        .iter()
        .filter(|ticket| {
            ticket_matches_view(ticket, store, view)
                && query::matches(ticket, tickets, filter, current_user)
        })
        .map(|ticket| ticket.id().to_owned())
        .collect::<HashSet<_>>();
    if !filter.trim().is_empty() {
        let matched = tickets
            .iter()
            .enumerate()
            .filter_map(|(index, ticket)| visible.contains(ticket.id()).then_some(index))
            .collect::<Vec<_>>();
        for index in matched {
            let ticket = &tickets[index];
            let mut parent = ticket.field("parent");
            let mut path = HashSet::new();
            while let Some(ticket) = by_id
                .get(parent)
                .copied()
                .filter(|candidate| path.insert(candidate.id()))
            {
                visible.insert(ticket.id().to_owned());
                parent = ticket.field("parent");
            }
        }
    }
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (index, ticket) in tickets
        .iter()
        .enumerate()
        .filter(|(_, ticket)| visible.contains(ticket.id()))
    {
        if !ticket.field("parent").is_empty()
            && visible.contains(ticket.field("parent"))
            && ticket.field("parent") != ticket.id()
        {
            children
                .entry(ticket.field("parent"))
                .or_default()
                .push(index);
        } else {
            roots.push(index);
        }
    }
    let sort = |items: &mut Vec<usize>| {
        items.sort_by(|a, b| {
            let a = &tickets[*a];
            let b = &tickets[*b];
            (a.priority(), a.id()).cmp(&(b.priority(), b.id()))
        })
    };
    sort(&mut roots);
    for items in children.values_mut() {
        sort(items);
    }
    let mut rows = Vec::new();
    let mut visited = HashSet::new();
    fn visit(
        ticket_index: usize,
        depth: usize,
        last: bool,
        tickets: &[Ticket],
        children: &HashMap<&str, Vec<usize>>,
        visited: &mut HashSet<String>,
        rows: &mut Vec<Row>,
    ) {
        let ticket = &tickets[ticket_index];
        if !visited.insert(ticket.id().to_owned()) {
            return;
        }
        rows.push(Row {
            ticket: ticket_index,
            depth,
            last,
        });
        if let Some(items) = children.get(ticket.id()) {
            let count = items.len();
            for (index, child) in items.iter().copied().enumerate() {
                visit(
                    child,
                    depth + 1,
                    index + 1 == count,
                    tickets,
                    children,
                    visited,
                    rows,
                );
            }
        }
    }
    let count = roots.len();
    for (index, root) in roots.into_iter().enumerate() {
        visit(
            root,
            0,
            index + 1 == count,
            tickets,
            &children,
            &mut visited,
            &mut rows,
        );
    }
    for (index, ticket) in tickets.iter().enumerate() {
        if visible.contains(ticket.id()) && !visited.contains(ticket.id()) {
            visit(index, 0, true, tickets, &children, &mut visited, &mut rows);
        }
    }
    rows
}

fn ticket_matches_view(ticket: &Ticket, store: &TicketStore, view: TicketView) -> bool {
    match view {
        TicketView::Active => ticket.status() != "closed",
        TicketView::All => true,
        TicketView::Blocked => {
            ticket.status() != "closed" && store.blocked_ids.contains(ticket.id())
        }
        TicketView::Closed => ticket.status() == "closed",
    }
}
