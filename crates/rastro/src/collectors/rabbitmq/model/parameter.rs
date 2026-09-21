//! One runtime parameter, whose value is never printed.

use rastro_collector::Observation;

/// A runtime parameter as the export carries it, with its value withheld.
///
/// **The whole value is sensitive and no component is trusted by name.** Measured: a shovel
/// keeps its peer's credentials inside `src-uri`
/// (`amqp://shovel-user:hunter2@upstream.example.com`) and a federation upstream inside `uri`.
/// This is the rule the container facet already applies to an environment variable, for the
/// same reason it gives: a plugin may define any component, so judging by name would be a
/// list that has to be right about software rastro has never seen.
///
/// **Withholding the whole value settles two other problems at once.** A parameter's value is
/// arbitrary JSON, and measured it arrives in two shapes: an object for a shovel, and an
/// Erlang proplist of two-element lists for a global parameter, one of whose numbers was
/// `0.5`. Carried as one text scalar there is no shape to interpret and no float can reach
/// the document, while the stand-in still changes whenever the parameter does.
///
/// **An operator policy is one of these, which is measured rather than assumed.** The export
/// has no `operator_policies` key: an operator policy arrives here with the component
/// `operator_policy`. That means its definition is withheld along with every other
/// parameter's, which is a real loss of signal for an entry that holds no credential, and it
/// is the safe direction to be wrong in until an allowlist has been argued for properly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// The value's own JSON spelling, withheld at render time.
    pub value: String,
}

impl From<&Parameter> for Observation {
    fn from(parameter: &Parameter) -> Self {
        Observation::object([(
            "value",
            Observation::text(parameter.value.as_str()).sensitive(),
        )])
    }
}
