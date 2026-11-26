// Le seul script de worthfold en dehors de htmx : après un envoi réussi, vide ce que le formulaire demande.
//   data-reset-on-success          : remet tout le formulaire à zéro ;
//   data-clear-on-success="amount" : vide seulement le champ nommé.
// Un script chargé depuis l'application (et non un attribut `hx-on`) reste compatible avec une CSP stricte.
document.addEventListener("htmx:afterRequest", (event) => {
  const form = event.target;
  if (!event.detail.successful || !(form instanceof HTMLFormElement)) return;
  if (form.hasAttribute("data-reset-on-success")) {
    form.reset();
  }
  const field = form.getAttribute("data-clear-on-success");
  if (field && form.elements[field]) {
    form.elements[field].value = "";
  }
});
