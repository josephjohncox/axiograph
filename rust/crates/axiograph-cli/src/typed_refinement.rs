//! Olog application adapter; shared refinement identities live in axiograph-query.
use anyhow::{anyhow, Result};
pub use axiograph_query::typed_refinement::*;

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_olog_refinement_handle(
    fragment: &crate::typed_authoring::OlogFragmentV1,
    handle: &RuntimeRefinementHandleV2,
) -> Result<crate::typed_authoring::OlogFragmentV1> {
    handle.validate()?;
    let RuntimeRefinementPayloadV2::OlogAuthoring { op } = &handle.payload else {
        return Err(anyhow!(
            "runtime refinement handle `{}` is not an olog authoring refinement",
            handle.id
        ));
    };
    let mut refined = fragment.clone();
    match op {
        OlogRefinementOpV1::BindRelationRole {
            relation_box,
            role,
            target_box,
        } => {
            let rel_box = refined
                .relation_boxes
                .iter_mut()
                .find(|candidate| candidate.box_id == *relation_box)
                .ok_or_else(|| anyhow!("unknown relation box `{relation_box}`"))?;
            if rel_box
                .role_bindings
                .iter()
                .any(|binding| binding.role == *role)
            {
                return Err(anyhow!(
                    "relation box `{relation_box}` already binds role `{role}`"
                ));
            }
            rel_box
                .role_bindings
                .push(crate::typed_authoring::OlogRoleBindingV1 {
                    role: role.clone(),
                    target_box: target_box.clone(),
                });
        }
        OlogRefinementOpV1::RetargetRelationRole {
            relation_box,
            role,
            target_box,
        } => {
            let rel_box = refined
                .relation_boxes
                .iter_mut()
                .find(|candidate| candidate.box_id == *relation_box)
                .ok_or_else(|| anyhow!("unknown relation box `{relation_box}`"))?;
            let binding = rel_box
                .role_bindings
                .iter_mut()
                .find(|binding| binding.role == *role)
                .ok_or_else(|| {
                    anyhow!("relation box `{relation_box}` does not bind role `{role}`")
                })?;
            binding.target_box = target_box.clone();
        }
    }
    Ok(refined)
}
