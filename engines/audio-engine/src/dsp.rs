// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! DSP : buffers, mathématiques partagées et frontière backend.
//!
//! [`SampleBuffer`] transporte des échantillons mono (`f32`) ; [`gain_linear`]
//! et [`mix_into`] sont les deux primitives partagées (vraies mathématiques,
//! testées) ; [`DspBackend`] traite nœud par nœud en ordre topo
//! ([`process_graph`]). Le routage des buffers inter-nœuds et l'audio I/O
//! appartiennent aux backends réels — pas de fausse machinerie ici.

use crate::{AudioGraph, AudioId, AudioNode};

/// Tampon d'échantillons mono.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleBuffer {
    /// Échantillons (1.0 = pleine échelle).
    pub data: Vec<f32>,
}

impl SampleBuffer {
    /// Silence (`frames` zéros).
    #[must_use]
    pub fn silence(frames: usize) -> Self {
        Self {
            data: vec![0.0; frames],
        }
    }

    /// Nombre d'échantillons.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Tranche de lecture.
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Tranche d'écriture.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }
}

/// Facteur linéaire pour un gain dB (`0 dB → 1`, `-6 dB → ~0.5`).
#[must_use]
pub fn gain_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Mixe `src` dans `dst` (addition, taille = min des deux — pas de
/// troncature silencieuse au-delà : l'appelant dimensionne).
pub fn mix_into(dst: &mut [f32], src: &[f32]) {
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d += *s;
    }
}

/// Backend DSP : traite les nœuds en ordre topo (voir [`process_graph`]).
/// Le routage des buffers et l'I/O appartiennent à l'implémentation.
pub trait DspBackend {
    /// Traite UN nœud pour `frames` échantillons (état interne conservé
    /// d'un appel à l'autre — réverbérations, enveloppes…).
    ///
    /// # Errors
    ///
    /// Retourne un message si le traitement échoue.
    fn process_node(&mut self, node: &AudioNode, frames: usize) -> Result<(), String>;
}

/// Bilan de [`process_graph`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessReport {
    /// Nœuds soumis au backend.
    pub processed: usize,
}

/// Soumet le graphe en ordre topo (amont d'abord), nœuds désactivés
/// INCLUS (le bypass est décidé par le backend, pas par l'ordre).
///
/// # Errors
///
/// Remonte la première erreur du backend.
pub fn process_graph<B: DspBackend>(
    backend: &mut B,
    graph: &AudioGraph,
    frames: usize,
) -> Result<ProcessReport, String> {
    let mut processed = 0;
    for id in graph.process_order() {
        if let Some(node) = graph.find(id) {
            backend.process_node(node, frames)?;
            processed += 1;
        }
    }
    Ok(ProcessReport { processed })
}

/// Backend nul : enregistre (id, frames), ne produit rien.
/// Gabarit des backends réels + tests sans carte son.
#[derive(Debug, Default)]
pub struct NullBackend {
    calls: Vec<(AudioId, usize)>,
}

impl NullBackend {
    /// Nouveau backend nul.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appels reçus, en ordre.
    #[must_use]
    pub fn calls(&self) -> &[(AudioId, usize)] {
        &self.calls
    }
}

impl DspBackend for NullBackend {
    fn process_node(&mut self, node: &AudioNode, frames: usize) -> Result<(), String> {
        self.calls.push((node.id, frames));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Wave;

    #[test]
    fn gain_et_mixage_mathematiques() {
        assert!((gain_linear(0.0) - 1.0).abs() < 0.001);
        assert!((gain_linear(-6.0) - 0.501).abs() < 0.002);
        assert!((gain_linear(6.0) - 1.995).abs() < 0.002);
        let mut dst = vec![1.0, 2.0, 3.0];
        mix_into(&mut dst, &[10.0, 20.0]);
        assert_eq!(dst, vec![11.0, 22.0, 3.0]);
        let silence = SampleBuffer::silence(4);
        assert_eq!(silence.as_slice(), &[0.0; 4]);
        assert!(!silence.is_empty());
        assert!(SampleBuffer::silence(0).is_empty());
    }

    #[test]
    fn driver_suit_l_ordre_topo() {
        let mut graph = AudioGraph::new();
        let src = graph.add_node(AudioNode::oscillator("osc", 440.0, Wave::Sine));
        let gain = graph.add_node(AudioNode::gain("g", 0.0));
        let out = graph.add_node(AudioNode::mixer("mix"));
        graph.connect(src, gain).expect("ok");
        graph.connect(gain, out).expect("ok");
        let mut backend = NullBackend::new();
        let report = process_graph(&mut backend, &graph, 128).expect("null ok");
        assert_eq!(report.processed, 3);
        assert_eq!(backend.calls(), &[(src, 128), (gain, 128), (out, 128)]);
    }
}
